import { ApiFailure, isDecimal, type Schema } from './api';

export type ExitPreview = Schema['CapitalExitPreviewV1'];
export type ExitView = Schema['CapitalExitViewV1'];
export type ExitAction = Schema['CapitalExitActionV1'];
export type ExitRequest =
  | { kind: 'PREVIEW'; project: string; source: string; body: Schema['CapitalExitPreviewRequestV1'] }
  | { kind: 'START'; project: string; source: string; body: Schema['CapitalExitStartV1'] }
  | { kind: 'ACTION'; project: string; source: string; id: string; body: ExitAction };
export type ExitResult = { kind: 'PREVIEW'; value: ExitPreview } | { kind: 'INTENT'; value: ExitView };
type Operation = { key: string; request: ExitRequest };
type State = { pending: boolean; unknown: boolean; operation?: Operation; result?: ExitResult; lastIntent?: string; error?: unknown };
type Storage = Pick<globalThis.Storage, 'getItem' | 'setItem'>;
const uuid = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;

/** A pending write survives same-tab refresh. Only this bounded non-secret workflow
 * request and original key are retained; no response, token or credential is stored.
 * Restoring never submits. The user explicitly reconciles the unchanged operation. */
export class ExitSession {
  private listeners = new Set<() => void>();
  private state: State = { pending: false, unknown: false };
  private completedRequest?: string;
  constructor(readonly storageKey: string, private storage?: Storage, binding?: { project: string; source: string }) {
    try {
      const text = storage?.getItem(storageKey);
      if (!text) return;
      if (text.length > 65536) throw new Error('invalid recovery record');
      const saved = JSON.parse(text);
      if (saved.version !== 1 || (saved.lastIntent !== undefined && !uuid.test(saved.lastIntent))) throw new Error('invalid recovery identity');
      if (saved.operation) {
        const { key, request } = saved.operation;
        if (!uuid.test(key) || !request || !uuid.test(request.project) || !uuid.test(request.source)
          || (binding && (request.project !== binding.project || request.source !== binding.source))
          || !['PREVIEW', 'START', 'ACTION'].includes(request.kind) || request.body?.schema_version !== 1
          || (request.kind === 'ACTION' && (!uuid.test(request.id) || !['PAUSE', 'CANCEL', 'RESUME', 'RECONCILE_WITHDRAWAL'].includes(request.body.action)))) throw new Error('invalid recovery operation');
      }
      this.state = { pending: false, unknown: !!saved.operation, operation: saved.operation, lastIntent: saved.lastIntent };
    } catch {
      // Never discard an unreadable potentially submitted operation and mint a new key.
      this.state = { pending: false, unknown: true, error: new ApiFailure('RECOVERY_UNAVAILABLE', '原操作恢复记录无法读取。请先通过 CLI 查询账户退出记录，勿再次开始退出。') };
    }
  }
  forgetCompletedView() {
    if (this.state.pending || this.state.unknown || this.state.result?.kind === 'PREVIEW') return false;
    try { this.persist(undefined, null); }
    catch (error) { this.update({ error }); return false; }
    this.completedRequest = undefined;
    this.update({ result: undefined, lastIntent: undefined, error: undefined });
    return true;
  }
  subscribe = (listener: () => void) => { this.listeners.add(listener); return () => { this.listeners.delete(listener); }; };
  getSnapshot = () => this.state;
  private update(changes: Partial<State>) { this.state = { ...this.state, ...changes }; this.listeners.forEach(listener => listener()); }
  private persist(operation: Operation | undefined, lastIntent: string | null | undefined = this.state.lastIntent) {
    if (!this.storage) throw new ApiFailure('LOCAL_VALIDATION_ERROR', '浏览器无法保留原操作标识；未提交。请使用支持恢复的浏览器或 CLI。');
    this.storage.setItem(this.storageKey, JSON.stringify({ version: 1, operation, lastIntent: lastIntent ?? undefined }));
  }
  async submit(request: ExitRequest | undefined, send: (request: ExitRequest, key: string) => Promise<ExitResult>) {
    if (this.state.pending || (this.state.unknown && !this.state.operation)) return;
    // A rejected retry cannot establish that the original uncertain write failed.
    // Capture the prior outcome before starting this attempt, including after reload.
    const retryingUnknown = this.state.unknown;
    // Unknown work always wins over new caller data. A successful identical click
    // is also absorbed until the user makes an explicit new request.
    let operation = this.state.operation;
    if (!operation) {
      if (!request) return;
      if (request.kind !== 'PREVIEW' && this.completedRequest === JSON.stringify(request)) return this.state.result;
      operation = { key: crypto.randomUUID(), request: structuredClone(request) };
      try { this.persist(operation); }
      catch (error) { this.update({ error }); return; }
    }
    this.update({ operation, pending: true, error: undefined });
    try {
      const result = await send(operation.request, operation.key);
      if (result.value.project_id !== operation.request.project || result.value.account_source_id !== operation.request.source
        || (operation.request.kind === 'ACTION' && result.value.id !== operation.request.id)) throw new ApiFailure('HTTP_CONTRACT_ERROR', '响应账户或退出标识不匹配；保留原请求核对。');
      const lastIntent = result.kind === 'INTENT' ? result.value.id : this.state.lastIntent;
      // A validated receipt still cannot erase the durable request if storage fails.
      // Replaying that exact key later is safe and retrieves the original receipt.
      this.persist(undefined, lastIntent);
      this.completedRequest = JSON.stringify(operation.request);
      this.update({ operation: undefined, pending: false, unknown: false, result, lastIntent });
      return result;
    } catch (error) {
      const rejected = !retryingUnknown && error instanceof ApiFailure && ((!!error.problem && error.status >= 400 && error.status < 500)
        || ['OFFLINE', 'LOCAL_VALIDATION_ERROR'].includes(error.code));
      if (rejected) {
        try { this.persist(undefined); }
        catch { this.update({ pending: false, unknown: true, error }); return; }
      }
      this.update({ pending: false, unknown: !rejected, operation: rejected ? undefined : operation, error });
    }
  }
}

/** A Drawer consumes each completion once. An explicit historical selection has
 * already superseded the result cached before it opened; new completions still
 * belong to the active account workflow. This cursor never clears shared work. */
export function createExitResultReader(initialIntent: string | undefined, initialResult: ExitResult | undefined) {
  let consumed = initialIntent === undefined ? undefined : initialResult;
  return (result: ExitResult | undefined) => {
    if (result === consumed) return undefined;
    consumed = result;
    return result;
  };
}

export function positiveDecimal(value: string) { return isDecimal(value) && !value.startsWith('-') && /[1-9]/.test(value); }
export function nonnegativeDecimal(value: string) { return isDecimal(value) && !value.startsWith('-'); }
export function money(value: Schema['AccountMoneyV1'] | null | undefined) { return value ? `${value.amount} ${value.currency}` : '暂无证据'; }
export function previewCanStart(preview: ExitPreview, now: number, observation?: string) {
  return preview.capability === 'SUPPORTED' && preview.expected_account_control_revision != null
    && Date.parse(preview.valid_until) > now && preview.original_observation_id === observation;
}
export function withdrawableDisplay(exit: ExitView, now: number, readCurrent = true) {
  const funds = exit.funds;
  const fresh = readCurrent && funds.evidence_asof != null && funds.evidence_valid_until != null
    && Date.parse(funds.evidence_asof) <= now && Date.parse(funds.evidence_valid_until) > now;
  if (exit.environment !== 'LIVE') return { label: '模拟可用现金', value: fresh && funds.withdrawability === 'SIMULATED' ? money(funds.verified_withdrawable_amount) : '暂无有效模拟证据', state: fresh ? 'SIMULATED' : 'STALE' };
  if (!fresh || funds.withdrawability === 'STALE') return { label: '已核验可提金额', value: '证据已过期或未刷新，当前不可确认', state: 'STALE' };
  if (funds.withdrawability !== 'VERIFIED' || funds.verified_withdrawable_amount == null) return { label: '已核验可提金额', value: '暂无证据', state: 'UNVERIFIED' };
  return { label: '已核验可提金额', value: money(funds.verified_withdrawable_amount), state: 'VERIFIED' };
}

// Display-only exact difference; this never supplies authority or execution size.
export function remainingReduction(requested: Schema['AccountMoneyV1'], idle: Schema['AccountMoneyV1'] | null | undefined) {
  if (!idle || requested.currency !== idle.currency || !nonnegativeDecimal(requested.amount) || !nonnegativeDecimal(idle.amount)) return '暂无证据';
  const parts = [requested.amount.split('.'), idle.amount.split('.')];
  const scale = Math.max(parts[0]![1]?.length ?? 0, parts[1]![1]?.length ?? 0);
  const integer = (part: string[]) => BigInt(part[0]! + (part[1] ?? '').padEnd(scale, '0'));
  const difference = integer(parts[0]!) - integer(parts[1]!);
  const digits = (difference > 0n ? difference : 0n).toString().padStart(scale + 1, '0');
  const decimal = scale ? `${digits.slice(0, -scale)}.${digits.slice(-scale)}`.replace(/0+$/, '').replace(/\.$/, '') : digits;
  return `${decimal || '0'} ${requested.currency}`;
}
