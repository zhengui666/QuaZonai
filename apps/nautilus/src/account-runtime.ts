import type {NativeSessionBinding} from '../../../packages/contracts/src/native-strategy-control.ts';
import type { NativeAccountStrategyConfig } from '../../../packages/contracts/src/account-strategy.ts';
import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process';
import { createInterface } from 'node:readline';
import { randomUUID } from 'node:crypto';
import { mkdirSync, openSync, writeSync, ftruncateSync, closeSync } from 'node:fs';
import { join } from 'node:path';
import type { NativeAccountRiskConfig, NativePaperExecutionConfig } from '../../../packages/contracts/src/nautilus.ts';

/** One configured physical account in the existing remote native worker. */
export interface AccountOptions {
  id: string; clientId: string; accountId: string; mode: 'paper' | 'live';
  nativeStrategy: NativeAccountStrategyConfig;
  instruments: string[]; accountAddress?: string; network?: 'mainnet' | 'testnet';
  /** Omit to retain all products during recovery. Narrow only after resolving out-of-scope orders. */
  executionProductTypes?: ('SPOT' | 'PERP' | 'OUTCOME')[];
  startingBalance?: string; cashBuffer?: string; paperFeeRate?: string;
  risk?: NativeAccountRiskConfig;
  paperExecution?: NativePaperExecutionConfig;
}
export interface NativeInstrument {
  instrument_id: string; base_currency: string; quote_currency: string;
  size_increment: string; price_increment: string; minimum_quantity: string | null; minimum_notional: string | null;
  taker_fee_rate: string | null; bid: string | null; ask: string | null;
  bid_depth: { price: string; quantity: string }[]; asof_ns: string | null;
}
export interface NativeOrder {
  trader_id?: string; strategy_id?: string; account_id?: string | null;
  is_active_local?: boolean; emulation_trigger?: string | null; exec_algorithm_id?: string | null;
  client_order_id: string; venue_order_id: string | null; instrument_id: string; side: string;
  quantity: string; filled_quantity: string; leaves_quantity: string; status: string;
  order_type?: string; trigger_price?: string | null; order_list_id?: string | null;
  parent_order_id?: string | null; contingency_type?: string | null; protection_target_id?: string | null;
  limit_price: string | null; reduce_only: boolean; exit_id: string | null;
}
export interface NativeFill {
  trade_id: string; client_order_id: string; instrument_id: string; side: string;
  quantity: string; price: string; commission: string | null; commission_currency: string | null; ts_event_ns: string;
}
export interface NativeAccountSnapshot {
  ready: boolean; recovery_required: boolean; reason?: string | null; sequence: string;
  native_version: string;
  observed_at_ns: string; observation_id: string; native_session_id: string; native_trader_id: string; native_account_id: string;
  native_snapshot: Record<string, unknown> | null;
  /** Independently timed official spot facts, never part of Portfolio sequence. */
  spot_balance_observation?: Record<string, unknown> | null;
  positions: { instrument_id: string; quantity: string; value: string | null; pnl: string | null }[];
  instruments: NativeInstrument[]; orders: NativeOrder[]; fills: NativeFill[];
  cash: string | null; equity: string | null; currency: string; currency_precision: number;
}
export interface NativeAccountRuntime {
  start?(): Promise<void>;
  paperCancellationBinding?(): NativeSessionBinding | null;
  paperCancellationCapability?(): Record<string,unknown>;
  strategyLifecycleCapability?(): Record<string,unknown>;
  bindPaperCancellationSession?(expectedSessionId: string): {request(operation: 'ORDER_CANCEL_ATTEMPT'|'ORDER_OBSERVE', payload: Record<string, unknown>): Promise<Record<string, unknown>>};
  isConnected?(): boolean;
  request(operation: string, payload?: Record<string, unknown>): Promise<Record<string, unknown>>;
  subscribeTicks?(listener: () => void): () => void;
  close(): Promise<void>;
}

/** NDJSON transport only. Business receipts are persisted before financial messages. */
export class AccountRuntime implements NativeAccountRuntime {
  private child: ChildProcessWithoutNullStreams | null = null;
  private starting: Promise<void> | null = null;
  private stopped = false;
  private nativeSessionId: string | null = null;
  private ordinaryVenueCancel = false;
  private cancellationFacts:Record<string,unknown> = {};
  private lifecycleFacts:Record<string,unknown>|null = null;
  private ticks = new Set<() => void>();
  private binary: string;
  private options: AccountOptions;
  private sourceId: string;
  private dataDir: string;
  private pending = new Map<string, { resolve(value: Record<string, unknown>): void; reject(error: Error): void; timer: NodeJS.Timeout }>();
  constructor(binary: string, options: AccountOptions, sourceId: string, dataDir: string) {
    this.binary = binary; this.options = options; this.sourceId = sourceId; this.dataDir = dataDir;
  }
  isConnected(): boolean { return this.child !== null || this.starting !== null; }
  async start(): Promise<void> {
    if (this.stopped) throw new Error('account_runtime_stopped');
    if (this.starting) return this.starting;
    if (this.child) return;
    this.starting = (async () => {
      const env: NodeJS.ProcessEnv = { PATH: process.env.PATH, TMPDIR: process.env.TMPDIR };
      for (const key of ['HTTP_PROXY', 'HTTPS_PROXY', 'ALL_PROXY', 'NO_PROXY', 'http_proxy', 'https_proxy', 'all_proxy', 'no_proxy',
        'SSL_CERT_FILE', 'SSL_CERT_DIR', 'CURL_CA_BUNDLE', 'REQUESTS_CA_BUNDLE', 'NODE_EXTRA_CA_CERTS']) if (process.env[key]) env[key] = process.env[key];
      // The scientific child environment remains isolated. Only this explicitly
      // configured account process receives its native venue credential names.
      if (this.options.mode === 'live') for (const key of ['HYPERLIQUID_PK', 'HYPERLIQUID_TESTNET_PK']) if (process.env[key]) env[key] = process.env[key];
      // A private, per-process bounded tail, never a public receipt/error.
      let diagnostic: number;
      try {
        mkdirSync(this.dataDir, { recursive: true, mode: 0o700 });
        diagnostic = openSync(join(this.dataDir, `native-stderr-${randomUUID()}.log`), 'wx', 0o600);
      } catch { throw new Error('account_runtime_not_started_diagnostic_unavailable'); }
      let tail: Buffer = Buffer.alloc(0); let diagnosticOpen = true;
      const child = spawn(this.binary, ['account-runtime'], { env, stdio: ['pipe', 'pipe', 'pipe'] }); this.child = child;
      const failed = () => {
        // A superseded process can emit error and then exit after its successor
        // has started. It has no authority over that generation's requests.
        if (this.child !== child) return;
        this.child = null; this.nativeSessionId = null; this.ordinaryVenueCancel = false; this.cancellationFacts = {}; this.lifecycleFacts = null;
        for (const item of this.pending.values()) { clearTimeout(item.timer); item.reject(new Error('account_runtime_outcome_uncertain')); }
        this.pending.clear();
      };
      child.once('error', failed); child.once('exit', failed);
      // Retain at most 64 KiB. SDK diagnostics never enter HTTP or receipts.
      child.stderr.on('data', (chunk: Buffer) => {
        tail = Buffer.from(Buffer.concat([tail, chunk.subarray(-65536)]).subarray(-65536));
        if (!diagnosticOpen) return;
        try { writeSync(diagnostic, tail, 0, tail.length, 0); ftruncateSync(diagnostic, tail.length); }
        catch { /* A log write failure cannot change an unknown trading outcome. */ }
      });
      child.once('close', () => { if (diagnosticOpen) { diagnosticOpen = false; try { closeSync(diagnostic); } catch { /* Preserve process outcome. */ } } });
      const lines = createInterface({ input: child.stdout });
      lines.on('line', line => {
        let message: { requestId?: string; result?: Record<string, unknown>; error?: string; event?: string };
        try { message = JSON.parse(line) as typeof message; } catch { return; }
        if (message.event === 'ACCOUNT_TICK') { for (const listener of this.ticks) listener(); return; }
        if (typeof message.requestId !== 'string') return;
        const item = this.pending.get(message.requestId); if (!item) return;
        this.pending.delete(message.requestId); clearTimeout(item.timer);
        if (typeof message.error === 'string') item.reject(new Error(message.error));
        else if (message.result && typeof message.result === 'object') item.resolve(message.result);
        else item.reject(new Error('account_runtime_response_invalid'));
      });
      try {
        const boot = await this.send('BOOT', { config: { client_id: this.options.clientId, account_id: this.options.accountId,
          mode: this.options.mode, account_address: this.options.accountAddress ?? null, testnet: this.options.network === 'testnet',
          execution_product_types: this.options.executionProductTypes ?? null,
          native_strategy: this.options.nativeStrategy,
          instruments: this.options.instruments, starting_balance: this.options.startingBalance ?? '0', paper_fee_rate: this.options.paperFeeRate ?? null,
          risk: this.options.risk ?? {}, paper_execution: this.options.paperExecution ?? null,
          source_id: this.sourceId, data_dir: this.dataDir } }, 60_000);
        this.nativeSessionId = typeof boot.native_session_id === 'string' ? boot.native_session_id : null;
        this.ordinaryVenueCancel = boot.ordinary_venue_cancel === true;
        const lifecycle=boot.strategy_lifecycle as Record<string,unknown>|undefined;
        this.lifecycleFacts=lifecycle && !Array.isArray(lifecycle)
          && lifecycle.configuredKind===this.options.nativeStrategy.strategy.kind
          && typeof lifecycle.supported==='boolean' && lifecycle.strategyVerified===false
          && lifecycle.scope==='INSTALLED_DISPATCH_ONLY'
          && (lifecycle.reason===null||typeof lifecycle.reason==='string') ? structuredClone(lifecycle):null;
        this.cancellationFacts = {scope:boot.ordinary_venue_cancel_scope??null,compatibilityDeclaration:boot.account_compatibility_declaration??null,compatibilityVerified:boot.account_compatibility_verified===false?false:null};
      } catch (error) {
        failed(); child.stdin.end(); child.kill('SIGTERM'); throw error;
      }
    })().finally(() => { this.starting = null; });
    return this.starting;
  }
  /** Session-bound adapter for the scoped owned-paper cancellation path.
   * Production cancellation remains unverified. Never starts/restarts a node.
   * Captures the actual BOOT session and child generation, not an arbitrary caller
   * claim. This is process identity only, NOT proof of exclusive Nautilus TLS/node
   * ownership or permission to trade. Native operations must independently check
   * the same session and original order before any dispatch.
   */
  strategyLifecycleCapability():Record<string,unknown> {
    const connected=!this.stopped&&!this.starting&&!!this.child&&this.child.exitCode===null&&this.child.signalCode===null&&!!this.nativeSessionId;
    return connected&&this.lifecycleFacts ? {...this.lifecycleFacts,nativeSessionId:this.nativeSessionId} :
      {supported:null,reason:'account_native_strategy_capability_unobserved',configuredKind:null,scope:null,strategyVerified:null,nativeSessionId:null};
  }
  paperCancellationCapability():Record<string,unknown> {
    const supported=this.paperCancellationBinding()!=null;
    return {supported,reason:supported?null:'native_cancel_original_session_unavailable',...(supported?this.cancellationFacts:{scope:null,compatibilityDeclaration:null,compatibilityVerified:null})};
  }
  paperCancellationBinding(): NativeSessionBinding | null {
    if (!this.ordinaryVenueCancel || this.options.mode !== 'paper' || this.stopped || this.starting
      || !this.child || this.child.exitCode !== null || this.child.signalCode !== null || !this.nativeSessionId) return null;
    return {sourceId:this.sourceId, buildId:this.binary, environmentId:'Sandbox', nativeSessionId:this.nativeSessionId};
  }
  bindPaperCancellationSession(expectedSessionId: string) {
    const child = this.child;
    if (!this.ordinaryVenueCancel || this.options.mode !== 'paper' || this.stopped || this.starting || !child
      || !expectedSessionId || this.nativeSessionId !== expectedSessionId) {
      throw new Error('account_cancel_original_session_unavailable');
    }
    return {
      request: (operation: 'ORDER_CANCEL_ATTEMPT' | 'ORDER_OBSERVE', payload: Record<string, unknown>) => {
        if (this.stopped || this.starting || this.child !== child || child.exitCode !== null
          || child.signalCode !== null || this.nativeSessionId !== expectedSessionId) {
          return Promise.reject(new Error('account_cancel_original_session_unavailable'));
        }
        // No await between generation check and the existing send. The Rust
        // session check is still required, and no failure authorizes a retry.
        return this.send(operation, { ...payload, expected_native_session_id: expectedSessionId });
      },
    };
  }
  private send(operation: string, payload: Record<string, unknown>, timeout = 15_000): Promise<Record<string, unknown>> {
    const child = this.child; if (!child) return Promise.reject(new Error('account_runtime_unavailable'));
    const requestId = randomUUID();
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => { this.pending.delete(requestId); reject(new Error('account_runtime_outcome_uncertain')); }, timeout); timer.unref();
      this.pending.set(requestId, { resolve, reject, timer });
      child.stdin.write(JSON.stringify({ ...payload, requestId, operation }) + '\n', error => {
        if (!error) return; clearTimeout(timer); this.pending.delete(requestId); reject(new Error('account_runtime_outcome_uncertain'));
      });
    });
  }
  async request(operation: string, payload: Record<string, unknown> = {}): Promise<Record<string, unknown>> {
    if (this.stopped) throw new Error('account_runtime_stopped');
    if (!this.child) throw new Error('account_runtime_not_started');
    if (this.starting) await this.starting;
    return this.send(operation, payload);
  }
  subscribeTicks(listener: () => void): () => void { this.ticks.add(listener); return () => { this.ticks.delete(listener); }; }
  async close(): Promise<void> {
    this.stopped = true;
    const child = this.child; if (!child) return;
    const exited = new Promise<void>(resolve => child.once('exit', () => resolve()));
    try { await this.send('SHUTDOWN', {}, 10_000); } catch { /* The persisted financial intents remain uncertain. */ }
    child.stdin.end();
    let timer: NodeJS.Timeout | undefined;
    const timeout = new Promise<void>(resolve => { timer = setTimeout(resolve, 10_000); timer.unref(); });
    await Promise.race([exited, timeout]); if (timer) clearTimeout(timer);
    if (child.exitCode === null && child.signalCode === null) child.kill('SIGTERM');
  }
}
