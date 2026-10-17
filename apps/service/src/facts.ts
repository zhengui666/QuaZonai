import { defineDoc } from '@earendil-works/pi-durable';
import type { JsonObject } from '@earendil-works/pi-durable';
import type { JsonValue } from '@earendil-works/chord';
import type { ObservationView, PortfolioStart, PortfolioView, SettingsView, ObservationStart } from '../../../packages/contracts/src/index.ts';
import type { ConnectionConfig } from './config.ts';

export const Application = defineDoc<{
  passwordHash: string | null;
  modelDeviceId: string | null;
  sessions: Record<string, { digest: string; expires: number; kind: string; name: string }>;
  settings: JsonObject | null;
  extensionEnabled: Record<string, boolean>;
  researchRequests: Record<string, { original: JsonValue; input: JsonObject; dataset?: JsonObject }>;
  portfolios: Record<string, JsonObject>;
  observations: Record<string, JsonObject>;
}>({
  kind: 'qz.application', version: 1, scope: 'session',
  initial: () => ({ passwordHash: null, modelDeviceId: null, sessions: {}, settings: null, extensionEnabled: {}, researchRequests: {}, portfolios: {}, observations: {} }),
});
export interface PortfolioFact {
  input: PortfolioStart; view: PortfolioView;
  evidence: JsonObject[]; weights?: string[]; policy: JsonObject; native: JsonObject | null; target: JsonObject | null;
}
export interface ObservationFact {
  input: ObservationStart; view: ObservationView; connection: ConnectionConfig;
  target: JsonObject; taskId: number; snapshot: JsonObject | null;
}
export function json(value: unknown): JsonObject { return JSON.parse(JSON.stringify(value)) as JsonObject; }
export function portfolio(value: unknown) { return value as PortfolioFact; }
export function observation(value: unknown) { return value as ObservationFact; }
export function settings(value: unknown): SettingsView {
  const stored = value as Partial<SettingsView> | null;
  return { defaultModel: stored?.defaultModel ?? null, thinkingLevel: stored?.thinkingLevel ?? null };
}
export class Problem extends Error {
  statusCode: number;
  constructor(error: string, statusCode = 400) { super(error); this.statusCode = statusCode; }
}
export function object(value: unknown, name = 'input'): JsonObject {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Problem(`${name}_required`);
  return value as JsonObject;
}
export function text(value: unknown, name: string): string {
  if (typeof value !== 'string' || !value.trim()) throw new Problem(`${name}_required`);
  return value;
}
export function counter(value: unknown, name: string): string {
  if (typeof value !== 'string' || !/^\d+$/.test(value)) throw new Problem(`${name}_counter_required`);
  return value;
}
export function decimal(value: unknown, name: string): string {
  if (typeof value !== 'string' || !/^-?\d+(?:\.\d+)?$/.test(value)) throw new Problem(`${name}_decimal_required`);
  return value;
}
export function decimalUnits(value: string): bigint {
  const negative = value.startsWith('-'); const [integer, fraction = ''] = (negative ? value.slice(1) : value).split('.');
  if (fraction.length > 18) throw new Problem('decimal_precision_unsupported');
  return (negative ? -1n : 1n) * BigInt(integer + fraction.padEnd(18, '0'));
}
export function nsTime(value: string): string {
  const ns = BigInt(counter(value, 'timestamp')); const seconds = ns / 1_000_000_000n;
  return new Date(Number(seconds * 1000n)).toISOString().replace('.000Z', `.${(ns % 1_000_000_000n).toString().padStart(9, '0')}Z`);
}
export const nowNs = () => (BigInt(Date.now()) * 1_000_000n).toString();
