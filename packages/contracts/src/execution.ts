/** Safe public scientific failure information; original diagnostics remain in Durable. */
export type ExecutionErrorCategory = 'model' | 'data' | 'request' | 'resource' | 'runtime' | 'cancelled' | 'unknown';
export interface ExecutionErrorDetail {
  code: string;
  category: ExecutionErrorCategory;
  message: string;
  action: string;
  operation?: string;
  stage?: 'compile' | 'execute' | 'decode' | 'validate';
}
