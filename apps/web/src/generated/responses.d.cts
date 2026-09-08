export declare function validateResponse(path: string, method: string, status: number, value: unknown, contentType?: string | null): boolean;
export declare function responseKind(path: string, method: string, status: number, contentType?: string | null): "json" | "binary" | "event-stream" | "empty" | undefined;
export declare function validateCostCurrency(value: unknown): boolean;
