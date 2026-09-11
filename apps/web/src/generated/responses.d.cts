// Generated from Rust OpenAPI. Do not edit.
export declare function validateResponse(path: string, method: string, status: number, value: unknown, contentType?: string | null): boolean;
export declare function responseKind(path: string, method: string, status: number, contentType?: string | null): "json" | "binary" | "event-stream" | "empty" | undefined;
export declare function validateCostCurrency(value: unknown): boolean;
export declare function validateBaseCurrency(value: unknown): boolean;
export declare function validateDecimal(value: unknown): boolean;
export declare function validateCostAmount(value: unknown): boolean;
export declare function validateNativeCatalogKey(value: unknown): boolean;
export declare function validateProblem(value: unknown): value is import("./api").components["schemas"]["Problem"];
