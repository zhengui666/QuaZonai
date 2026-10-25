/** A string is an explicit local executable for the independent worker and
 * scientific tests. Application configuration admits remote targets only. */
export type RemoteNativeTarget = { id: string; url: string; tokenEnv?: string };
export type NativeTarget = string | RemoteNativeTarget;

export type NativeJson = null | boolean | number | string | NativeJson[] | { [key: string]: NativeJson };
export interface NativeCapabilities {
  schemaVersion: 1;
  implementationDigest: string;
  commands: string[];
}
export interface NativeExecutionRequest {
  schemaVersion: 1;
  requestId: string;
  /** Fresh for each scientific invocation; receipt replay retains this value. */
  invocationId: string;
  implementationDigest: string;
  command: string;
  input: NativeJson;
  args: string[];
  /** Base64 artifacts; paths in args use @file:name or @objects:prefix. */
  files: Record<string, string>;
}
export type NativeExecutionReceipt = {
  schemaVersion: 1;
  requestId: string;
} & (
  { status: 'COMPLETED'; result: NativeJson } |
  { status: 'FAILED' | 'UNKNOWN'; error: string } |
  { status: 'RUNNING' | 'CANCELLED' }
);
