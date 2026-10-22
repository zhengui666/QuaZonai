import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { JsonObject } from '@earendil-works/pi-durable';
import { accountKey, recordCapitalControl } from '../src/capital-exit.ts';
import type { AccountControl } from '../src/capital-exit.ts';
import { captureAccount } from '../src/pm-account.ts';
import { json, nowNs } from '../src/facts.ts';
import { accountSourceFixture } from './pm-account-fixture.ts';

test('PM account transport capability changes invalidate the retained basis without renewing unchanged receipt clocks', () => {
  const f = accountSourceFixture();
  const key = accountKey(f.connection);
  const application = f.application as { observations: Record<string, JsonObject>; accountControls: Record<string, JsonObject> };
  const previous = application.accountControls[key] as unknown as AccountControl;
  const fresh = { status: previous.status!, native: {}, capabilities: {
    clientId: f.connection.clientId, accountId: f.connection.accountId, mode: f.connection.mode, currency: 'USDC',
    automaticTargets: { supported: true, revisionBound: true },
  } };
  recordCapitalControl(application.accountControls, f.connection, fresh);
  const first = application.accountControls[key] as unknown as AccountControl;
  const captured = captureAccount(application, f.connection.id);
  recordCapitalControl(application.accountControls, f.connection, fresh);
  const repeat = application.accountControls[key] as unknown as AccountControl;
  assert.equal(repeat.capabilitiesReceivedAtNs, first.capabilitiesReceivedAtNs);
  application.accountControls[key] = json(repeat);
  assert.deepEqual(captureAccount(application, f.connection.id).basis, captured.basis);
  recordCapitalControl(application.accountControls, f.connection, { ...fresh, capabilities: { ...fresh.capabilities, automaticTargets: { supported: false, revisionBound: false } } });
  const changed = application.accountControls[key] as unknown as AccountControl;
  const after = captureAccount(application, f.connection.id);
  assert.notEqual(after.basis.positionsEvidenceRef, captured.basis.positionsEvidenceRef);
  assert.equal(after.basis.observedAtNs, captured.basis.observedAtNs);
  assert.equal(after.basis.sequence, captured.basis.sequence);
  assert.equal(after.basis.equity, null);
  assert.deepEqual(after.artifact.capitalCapabilities, { document: changed.capabilities, receivedAtNs: changed.capabilitiesReceivedAtNs });
  assert.ok(BigInt(after.basis.availableAtNs) <= BigInt(nowNs()));
});

test('PM transport capability evidence cannot become available before its original receiver clock', () => {
  const f = accountSourceFixture();
  const application = f.application as { observations: Record<string, JsonObject>; accountControls: Record<string, JsonObject> };
  application.accountControls[accountKey(f.connection)]!.capabilities = { clientId: f.connection.clientId, accountId: f.connection.accountId,
    mode: f.connection.mode, currency: 'USDC', automaticTargets: { supported: true, revisionBound: true } };
  application.accountControls[accountKey(f.connection)]!.capabilitiesReceivedAtNs = '111';
  assert.throws(() => captureAccount(application, f.connection.id, '110'), /capital_capabilities_receipt_clock_invalid/);
});
