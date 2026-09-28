import { Alert, Button } from 'antd';
import { useSyncExternalStore } from 'react';
import { ApiFailure, Intent } from './api';
import { setSettingsWork } from './settings-work';
import { useOnline } from './ui';

type CommandState = { pending: boolean; unknown: boolean; error?: unknown };
const commands = new Map<string, SettingsCommand>();
const listeners = new Set<() => void>();
let recoverable: SettingsCommand[] = [];
const empty: SettingsCommand[] = [];

class SettingsCommand {
  readonly intent = new Intent();
  readonly subscribe = (listener: () => void) => { this.listeners.add(listener); return () => { this.listeners.delete(listener); }; };
  readonly getSnapshot = () => this.state;
  state: CommandState = { pending: false, unknown: false };
  private listeners = new Set<() => void>();
  private request?: () => Promise<unknown>;
  private complete?: () => Promise<void> | void;
  constructor(readonly key: string, readonly label: string) {}
  private update(changes: Partial<CommandState>) {
    this.state = { ...this.state, ...changes };
    this.listeners.forEach(listener => listener());
    setSettingsWork(`command:${this.key}`, this.state.pending || this.state.unknown);
    recoverable = [...commands.values()].filter(command => command.state.unknown);
    listeners.forEach(listener => listener());
  }
  async submit(request: () => Promise<unknown>, complete: () => Promise<void> | void) {
    if (this.state.pending) return;
    if (!this.state.unknown) { this.request = request; this.complete = complete; }
    this.update({ pending: true, error: undefined });
    try {
      await this.request!();
    } catch (error) {
      const rejected = error instanceof ApiFailure && ((!!error.problem && error.status >= 400 && error.status < 500)
        || ['OFFLINE', 'LOCAL_VALIDATION_ERROR'].includes(error.code));
      if (rejected) { this.request = undefined; this.complete = undefined; this.intent.clear(); }
      this.update({ pending: false, unknown: !rejected, error });
      return;
    }
    const done = this.complete;
    this.request = undefined; this.complete = undefined; this.intent.clear();
    this.update({ unknown: false });
    try { await done?.(); this.update({ pending: false }); }
    catch (error) { this.update({ pending: false, error }); }
  }
  retry() { if (this.state.unknown && this.request) void this.submit(this.request, this.complete ?? (() => {})); }
}

export function useSettingsCommand(key: string, label: string) {
  let command = commands.get(key);
  if (!command) { command = new SettingsCommand(key, label); commands.set(key, command); }
  return { command, state: useSyncExternalStore(command.subscribe, command.getSnapshot, command.getSnapshot) };
}

export function SettingsCommandRecovery() {
  const online = useOnline();
  const pending = useSyncExternalStore(listener => {
    listeners.add(listener); return () => { listeners.delete(listener); };
  }, () => recoverable, () => empty);
  return pending.map(command => <Alert key={command.key} showIcon type="warning" title={`${command.label}结果待确认`}
    action={<Button disabled={!online || command.state.pending} onClick={() => command.retry()}>重试当前操作</Button>} />);
}
