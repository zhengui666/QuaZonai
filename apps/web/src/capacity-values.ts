/** Display an original Unix-nanosecond clock without converting the full counter to Number. */
export function nanosecondTimestamp(value: string): string | undefined {
  if (!/^\d+$/.test(value)) return;
  const clock = BigInt(value); const date = new Date(Number(clock / 1_000_000n));
  if (!Number.isFinite(date.getTime())) return;
  return date.toISOString().replace(/\.\d{3}Z$/, `.${(clock % 1_000_000_000n).toString().padStart(9, '0')}Z`);
}
