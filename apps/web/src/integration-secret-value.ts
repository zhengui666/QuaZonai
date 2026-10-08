// Local field validation mirrors the native purpose rules. PEM trust-anchor
// validation remains mandatory on the server before registration.
export function integrationSecretValueValid(purpose: 'RUNTIME' | 'DOWNSTREAM' | 'TLS_CA', value: string): boolean {
  return purpose === 'TLS_CA'
    ? value.length >= 1 && /^[\x00-\x7f]+(?![\s\S])/.test(value)
    : value.length >= (purpose === 'RUNTIME' ? 32 : 1) && value.length <= 8192 && /^[!-~]+(?![\s\S])/.test(value);
}
