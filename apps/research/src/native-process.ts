import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process';
import { Readable } from 'node:stream';
import { pipeline } from 'node:stream/promises';
import { jsonChunks } from './native-json.ts';
import type { NativeJson } from '../../../packages/contracts/src/nautilus.ts';

/** Live process handles are not a ledger. Only the existing Pi task owns durable outcome facts. */
const active = new Map<string, { child: ChildProcessWithoutNullStreams; exited: Promise<{code:number|null;signal:NodeJS.Signals|null}>; markTermination: () => void; markCleanup: () => void }>();
export type NativeProcessFacts = {
  errorCategory: string; exitObserved: boolean; exitCode: number | null;
  signal: string | null;
  /** Requests attempted, not evidence of delivery or descendant termination. */
  terminationRequested: boolean; cleanupSignalRequested: boolean;
};
export class NativeProcessFailure extends Error {
  readonly processFacts: NativeProcessFacts;
  constructor(message: string, processFacts: NativeProcessFacts, diagnostic: string) {
    super(message);
    this.processFacts=processFacts;
    Object.defineProperty(this, 'nativeDiagnostic', {value:diagnostic,enumerable:false});
  }
}
export type NativeProcessParser = { write(chunk:string): unknown | Promise<unknown>; finish(): NativeJson | Promise<NativeJson> };
export async function runNativeProcess(requestId: string, binary: string, command: string, args: string[], input: NativeJson,
  parser: NativeProcessParser, signal?: AbortSignal): Promise<NativeJson> {
  if (signal?.aborted) throw signal.reason;
  if (active.has(requestId)) throw new Error('native_process_original_already_running');
  const child = spawn(binary, [command,...args], {detached:process.platform !== 'win32',stdio:['pipe','pipe','pipe'],env:{PATH:process.env.PATH,TMPDIR:process.env.TMPDIR}});
  let startupError: Error | undefined;
  let observedExit: {code:number|null;signal:NodeJS.Signals|null}|undefined;
  let terminationRequested=false,cleanupSignalRequested=false;
  const exited = new Promise<{code:number|null;signal:NodeJS.Signals|null}>(resolve=>{
    child.once('error',error=>{startupError=error;resolve({code:null,signal:null});});
    child.once('exit',(code,signal)=>{observedExit={code,signal};resolve(observedExit);});
  });
  active.set(requestId,{child,exited,markTermination:()=>{terminationRequested=true;},markCleanup:()=>{cleanupSignalRequested=true;}});
  // Do not put native diagnostics or possible user data into public errors.
  let diagnostic=Buffer.alloc(0);
  child.stderr.on('data',(chunk:Buffer)=>{ diagnostic=Buffer.concat([diagnostic,chunk]).subarray(-65536); });
  let killTimer: ReturnType<typeof setTimeout> | undefined;
  const abort = () => { terminationRequested=true; signalProcessGroup(child,'SIGTERM'); killTimer ??= setTimeout(()=>{signalProcessGroup(child,'SIGKILL');child.stdout.destroy();child.stdin.destroy();},1000); killTimer.unref(); };
  signal?.addEventListener('abort',abort,{once:true});
  const writing = pipeline(Readable.from(jsonChunks(input)),child.stdin).catch(error=>error as Error);
  const decoder = new TextDecoder();
  try {
    for await (const chunk of child.stdout) await parser.write(decoder.decode(chunk as Buffer,{stream:true}));
    await parser.write(decoder.decode());
    const exit = await exited; const inputError=await writing;
    if (signal?.aborted) throw new Error('native_process_interrupted');
    if (startupError) throw new Error('native_process_launch_failed');
    if (exit.signal !== null) throw new Error('native_process_exit_unknown');
    if (exit.code !== 0) throw new Error('native_process_failed');
    if (inputError instanceof Error) throw new Error('native_process_input_failed');
    return await parser.finish();
  } catch(error) {
    // A parser/consumer failure cannot leave its original process producing unobserved output.
    if(!startupError&&!observedExit)abort(); await Promise.race([exited,new Promise(resolve=>setTimeout(resolve,2000))]); cleanupSignalRequested=true; signalProcessGroup(child,'SIGKILL');child.stdout.destroy();child.stdin.destroy();await writing;
    const categories = new Set(['native_process_interrupted','native_process_launch_failed','native_process_exit_unknown','native_process_failed','native_process_input_failed']);
    const category = error instanceof Error && categories.has(error.message) ? error.message : 'native_process_output_invalid';
    throw new NativeProcessFailure(category, {errorCategory:category,exitObserved:observedExit!==undefined,
      exitCode:observedExit?.code??null,signal:observedExit?.signal??null,terminationRequested,cleanupSignalRequested},diagnostic.toString('utf8'));
  } finally {
    child.stderr.destroy();
    signal?.removeEventListener('abort',abort); if(killTimer)clearTimeout(killTimer);
    if(active.get(requestId)?.child===child)active.delete(requestId);
  }
}
/** Never invent cancellation for a process whose original handle was lost. */
export async function cancelNativeProcess(requestId: string): Promise<'PROCESS_EXITED_BY_SIGNAL'|'UNKNOWN'> {
  const running=active.get(requestId); if(!running)return 'UNKNOWN';
  running.markTermination(); signalProcessGroup(running.child,'SIGTERM'); const kill=setTimeout(()=>{signalProcessGroup(running.child,'SIGKILL');running.child.stdout.destroy();running.child.stdin.destroy();},1000);kill.unref();
  const deadline=new Promise<null>(resolve=>setTimeout(()=>resolve(null),2000));
  try { const outcome=await Promise.race([running.exited,deadline]); if(!outcome)return 'UNKNOWN'; running.markCleanup(); signalProcessGroup(running.child,'SIGKILL');running.child.stdout.destroy();running.child.stdin.destroy(); return outcome.signal==='SIGTERM' || outcome.signal==='SIGKILL' ? 'PROCESS_EXITED_BY_SIGNAL' : 'UNKNOWN'; } finally {clearTimeout(kill);}
}
export function nativeProcessRunning(requestId:string) { return active.has(requestId); }

function signalProcessGroup(child:ChildProcessWithoutNullStreams,signal:NodeJS.Signals) {
  try { if(process.platform !== 'win32' && child.pid)process.kill(-child.pid,signal);else child.kill(signal); } catch { /* Loss of the handle never establishes task cancellation. */ }
}
