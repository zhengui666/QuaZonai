import { describe, expect, it } from 'vitest';
import type { Schema } from './api';
import { activeAccountOperation, liveLoginCode } from './chatgpt-auth';

const now = Date.parse('2026-09-25T00:00:00Z');
const operation: Schema['CodexAccountOperationV1'] = {
  schema_version: 1, revision: '9007199254740993', state: 'WAITING', updated_at: new Date(now).toISOString(),
  operation: { id: '01990000-0000-7000-8000-000000000001', profile_id: '01990000-0000-7000-8000-000000000002',
    profile_revision: '1', action: 'LOGIN', created_at: new Date(now).toISOString(), deadline_at: new Date(now + 900_000).toISOString() },
  finished_at: null, reason: null, account: null,
};
// Synthetic presentation sample, never a real device code or OAuth response.
const challenge = { id: operation.operation.id, code: { user_code: 'TEST-ONLY', verification_url: 'https://auth.openai.com/codex/device' } };

describe('native account progress and one-time code disclosure', () => {
  it.each(['REQUESTED', 'WAITING', 'CANCEL_REQUESTED'] as const)('keeps %s pending until the native result arrives', state => {
    expect(activeAccountOperation({ ...operation, state })).toBe(true);
  });
  it.each(['SUCCEEDED', 'CANCELLED', 'FAILED', 'UNKNOWN'] as const)('retires the code at %s', state => {
    const terminal = { ...operation, state };
    expect(activeAccountOperation(terminal)).toBe(false);
    expect(liveLoginCode(challenge, terminal, now)).toBeUndefined();
  });
  it('discloses only the initiating, still-waiting login code before its local deadline', () => {
    expect(liveLoginCode(challenge, operation, now)).toEqual(challenge.code);
    expect(liveLoginCode(challenge, operation, now + 900_000)).toBeUndefined();
    expect(liveLoginCode(challenge, { ...operation, state: 'REQUESTED' }, now)).toBeUndefined();
    expect(liveLoginCode(challenge, { ...operation, state: 'CANCEL_REQUESTED' }, now)).toBeUndefined();
    expect(liveLoginCode(challenge, { ...operation, operation: { ...operation.operation, action: 'LOGOUT' } }, now)).toBeUndefined();
    expect(liveLoginCode({ ...challenge, id: 'another-operation' }, operation, now)).toBeUndefined();
    expect(liveLoginCode(undefined, operation, now)).toBeUndefined();
    expect(liveLoginCode(challenge, null, now)).toBeUndefined();
    expect(activeAccountOperation(null)).toBe(false);
  });
  it.each(['javascript:alert(1)', 'data:text/html,login', 'http://auth.openai.com/codex/device',
    'https://name:password@auth.openai.com/codex/device', '//auth.openai.com/codex/device', 'invalid'])('rejects unsafe authorization link %s', verification_url => {
    expect(liveLoginCode({ ...challenge, code: { ...challenge.code, verification_url } }, operation, now)).toBeUndefined();
  });
});
