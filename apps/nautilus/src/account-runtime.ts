import type { NativeAccountStrategyConfig } from '../../../packages/contracts/src/account-strategy.ts';
import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process';
import { createInterface } from 'node:readline';
import { randomUUID } from 'node:crypto';
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
  positions: { instrument_id: string; quantity: string; free_quantity: string; value: string | null; pnl: string | null }[];
  instruments: NativeInstrument[]; orders: NativeOrder[]; fills: NativeFill[];
  cash: string | null; equity: string | null; currency: string; currency_precision: number;
}
export interface NativeAccountRuntime {
  request(operation: string, payload?: Record<string, unknown>): Promise<Record<string, unknown>>;
  subscribeTicks?(listener: () => void): () => void;
  close(): Promise<void>;
}

/** NDJSON transport only. Business receipts are persisted before financial messages. */
export class AccountRuntime implements NativeAccountRuntime {
  private child: ChildProcessWithoutNullStreams | null = null;
  private starting: Promise<void> | null = null;
  private stopped = false;
  private ticks = new Set<() => void>();
  private binary: string;
  private options: AccountOptions;
  private sourceId: string;
  private dataDir: string;
  private pending = new Map<string, { resolve(value: Record<string, unknown>): void; reject(error: Error): void; timer: NodeJS.Timeout }>();
  constructor(binary: string, options: AccountOptions, sourceId: string, dataDir: string) {
    this.binary = binary; this.options = options; this.sourceId = sourceId; this.dataDir = dataDir;
  }
  private async start(): Promise<void> {
    if (this.stopped) throw new Error('account_runtime_stopped');
    if (this.starting) return this.starting;
    if (this.child) return;
    this.starting = (async () => {
      const env: NodeJS.ProcessEnv = { PATH: process.env.PATH, TMPDIR: process.env.TMPDIR };
      for (const key of ['HTTP_PROXY', 'HTTPS_PROXY', 'ALL_PROXY', 'NO_PROXY', 'http_proxy', 'https_proxy', 'all_proxy', 'no_proxy',
        'SSL_CERT_FILE', 'SSL_CERT_DIR', 'CURL_CA_BUNDLE', 'REQUESTS_CA_BUNDLE', 'NODE_EXTRA_CA_CERTS']) if (process.env[key]) env[key] = process.env[key];
      // The scientific child environment remains isolated. Only this explicitly
      // configured account process receives its native venue credential names.
      for (const key of ['HYPERLIQUID_PK', 'HYPERLIQUID_TESTNET_PK']) if (process.env[key]) env[key] = process.env[key];
      const child = spawn(this.binary, [], { env, stdio: ['pipe', 'pipe', 'pipe'] }); this.child = child;
      const failed = () => {
        // A superseded process can emit error and then exit after its successor
        // has started. It has no authority over that generation's requests.
        if (this.child !== child) return;
        this.child = null;
        for (const item of this.pending.values()) { clearTimeout(item.timer); item.reject(new Error('account_runtime_outcome_uncertain')); }
        this.pending.clear();
      };
      child.once('error', failed); child.once('exit', failed);
      // Native logging belongs on stderr. Do not relay credential-bearing SDK
      // diagnostics into HTTP errors or user-visible receipts.
      child.stderr.resume();
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
        await this.send('BOOT', { config: { client_id: this.options.clientId, account_id: this.options.accountId,
          mode: this.options.mode, account_address: this.options.accountAddress ?? null, testnet: this.options.network === 'testnet',
          execution_product_types: this.options.executionProductTypes ?? null,
          native_strategy: this.options.nativeStrategy,
          instruments: this.options.instruments, starting_balance: this.options.startingBalance ?? '0', paper_fee_rate: this.options.paperFeeRate ?? null,
          risk: this.options.risk ?? {}, paper_execution: this.options.paperExecution ?? null,
          source_id: this.sourceId, data_dir: this.dataDir } }, 60_000);
      } catch (error) {
        failed(); child.stdin.end(); child.kill('SIGTERM'); throw error;
      }
    })().finally(() => { this.starting = null; });
    return this.starting;
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
    await this.start(); return this.send(operation, payload);
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
