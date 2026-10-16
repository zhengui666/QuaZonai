// Generated from Rust OpenAPI. Do not edit.
export declare class ResponseValidatorLoadError extends Error { constructor(moduleId: string, cause?: unknown); readonly moduleId: string; }
export declare function validateResponseAsync(path: string, method: string, status: number, value: unknown, contentType?: string | null): Promise<boolean>;
export declare function validateResponseResultAsync(path: string, method: string, status: number, value: unknown, contentType?: string | null): Promise<{ valid: boolean; errors: import("ajv").ErrorObject[] | null }>;
