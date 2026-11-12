import type { NativeTarget } from '../../../packages/contracts/src/nautilus.ts';
import type { Context } from '@earendil-works/chord';
import type { JsonObject } from '@earendil-works/pi-durable';
import { ExecutionFailure } from '../../research/src/execution-errors.ts';
import { executeNative } from '../../research/src/native-execution.ts';
import { Problem } from './facts.ts';

export async function nativeJob(binary: NativeTarget, command: string, input: JsonObject, args: string[], directory: string, context: Context): Promise<JsonObject> {
  try { return await executeNative(binary, command, input, args, directory, context); }
  catch (error) { throw error instanceof ExecutionFailure ? error : new Problem(error instanceof Error ? error.message : 'native_job_failed'); }
}
