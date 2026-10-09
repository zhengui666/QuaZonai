import { Alert, Button, Typography } from 'antd';
import { Fragment, useEffect, useRef, useSyncExternalStore } from 'react';
import { ApiFailure, Intent } from './api';
import { setSettingsWork } from './settings-work';
import { useOnline } from './ui';

type CommandState = { pending: boolean; unknown: boolean; receipts: string[]; error?: unknown };
const commands = new Map<string, SettingsCommand>();
const listeners = new Set<() => void>();
let recoverable: SettingsCommand[] = [];
const empty: SettingsCommand[] = [];

export class SettingsCommand {
  readonly intent = new Intent();
  readonly subscribe = (listener: () => void) => { this.listeners.add(listener); return () => { this.listeners.delete(listener); }; };
  readonly getSnapshot = () => this.state;
  state: CommandState = { pending: false, unknown: false, receipts: [] };
  private listeners = new Set<() => void>();
  private request?: () => Promise<string>;
  private complete?: () => Promise<void> | void;
  private mountedComplete?: () => Promise<void> | void;
  constructor(readonly key: string, readonly label: string) {}
  bindComplete(complete: () => Promise<void> | void) {
    this.mountedComplete = complete;
    return () => { if (this.mountedComplete === complete) this.mountedComplete = undefined; };
  }
  private update(changes: Partial<CommandState>) {
    this.state = { ...this.state, ...changes };
    this.listeners.forEach(listener => listener());
    setSettingsWork(`command:${this.key}`, this.state.pending || this.state.unknown || this.state.receipts.length > 0 || !!this.state.error);
    recoverable = [...commands.values()].filter(command => command.state.unknown || command.state.receipts.length > 0 || !!command.state.error);
    listeners.forEach(listener => listener());
  }
  async submit(request: () => Promise<string>, complete: () => Promise<void> | void) {
    if (this.state.pending) return;
    const wasUnknown = this.state.unknown;
    if (!wasUnknown) { this.request = request; this.complete = complete; }
    this.update({ pending: true, error: undefined });
    let receipt: string;
    try {
      receipt = await this.request!();
    } catch (error) {
      // A retry rejection cannot establish the outcome of the original request.
      const rejected = !wasUnknown && error instanceof ApiFailure && ((!!error.problem && error.status >= 400 && error.status < 500)
        || ['OFFLINE', 'LOCAL_VALIDATION_ERROR'].includes(error.code));
      if (rejected) { this.request = undefined; this.complete = undefined; this.intent.clear(); }
      this.update({ pending: false, unknown: !rejected, error });
      return;
    }
    const done = this.mountedComplete ?? this.complete;
    this.request = undefined; this.complete = undefined; this.intent.clear();
    this.update({ unknown: false, receipts: this.state.receipts.includes(receipt) ? this.state.receipts : [...this.state.receipts, receipt] });
    try { await done?.(); this.update({ pending: false }); }
    catch (error) { this.update({ pending: false, error }); }
  }
  retry() { if (this.state.unknown && this.request) void this.submit(this.request, this.complete ?? (() => {})); }
  acknowledge(receipt: string) { this.update({ receipts: this.state.receipts.filter(value => value !== receipt) }); }
  acknowledgeError() { this.update({ error: undefined }); }
}

export function useSettingsCommand(key: string, label: string, complete?: () => Promise<void> | void) {
  let command = commands.get(key);
  if (!command) { command = new SettingsCommand(key, label); commands.set(key, command); }
  const latest = useRef(complete); latest.current = complete;
  const mounted = !!complete;
  useEffect(() => mounted ? command.bindComplete(() => latest.current?.()) : undefined, [command, mounted]);
  return { command, state: useSyncExternalStore(command.subscribe, command.getSnapshot, command.getSnapshot) };
}

export function SettingsCommandRecovery() {
  const online = useOnline();
  const pending = useSyncExternalStore(listener => {
    listeners.add(listener); return () => { listeners.delete(listener); };
  }, () => recoverable, () => empty);
  return pending.map(command => <Fragment key={command.key}>
    {command.state.unknown && <Alert showIcon type="warning" title={`${command.label}结果待确认`}
      action={<Button disabled={!online || command.state.pending} onClick={() => command.retry()}>重试当前操作</Button>} />}
    {!command.state.unknown && !command.state.pending && !!command.state.error && <Alert showIcon type="error" title={`${command.label}未完成`}
      description={command.state.error instanceof ApiFailure ? command.state.error.message : '请求失败，请核对操作结果。'}
      action={<Button onClick={() => command.acknowledgeError()}>关闭错误</Button>} />}
    {command.state.receipts.map(receipt => <Alert key={receipt} showIcon type="success" title={`${command.label}回执已确认`}
      description={<Typography.Text copyable>{receipt}</Typography.Text>}
      action={<Button onClick={() => command.acknowledge(receipt)}>关闭回执</Button>} />)}
  </Fragment>);
}
