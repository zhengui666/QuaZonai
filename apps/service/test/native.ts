import assert from 'node:assert/strict';
import './capital-native.ts';
import './pm-science-native.ts';
import './pm-independent-policy.test.ts';
import './pm-review-consumer.test.ts';
import './pm-method.test.ts';
import './pm-account-native.test.ts';
import './pm-account-http.test.ts';
import './pm-forward.native.test.ts';
import './pm-forward-qualification.test.ts';
import './pm-forward-feasibility.test.ts';
import './pm-forward-http.test.ts';
import './pm-paper-receiver.test.ts';
import './pm-campaign-account.test.ts';
import './pm-source-recovery.test.ts';
import './pm-outcome-receipts.test.ts';
import './pm-operations.test.ts';
import './pm-business-acceptance.test.ts';
import { test } from 'node:test';
import type { JsonObject } from '@earendil-works/pi-durable';
import { context } from './support.ts';
import { runReviewScience } from '../../research/src/review-science-adapter.ts';
import { canonicalJsonDigest } from '../../research/src/native-json.ts';

test('review diagnostics bridge binds synthetic original source through real Rust stdout and TS projection', { timeout: 30_000 }, async () => {
  assert.ok(process.env.QZ_JOB_BINARY, 'QZ_JOB_BINARY must name the existing scientific job');
  for (const count of [128, 20]) {
    // Synthetic protocol evidence only: no market history, source qualification,
    // model review or future profitability is established by these returns.
    const baseline = Array.from({ length: count }, (_, index) => index % 2 === 0 ? 0.02 : -0.01);
    const analysis: JsonObject = { kind: 'PORTFOLIO', candidate: { id: 'synthetic-candidate', returns: baseline.map(value => value - 0.003) },
      baseline: { id: 'synthetic-baseline', returns: baseline }, timestamps_ns: baseline.map((_, index) => String(BigInt(index) * 86_400_000_000_000n)),
      frequency: 'DAY', block_length: 2 };
    const source: JsonObject = Object.freeze({ fixture: 'synthetic-original-shared-account-returns', analysis });
    const frozen = JSON.stringify(source); const evidenceRef = `fixture:original:${count}`;
    const result = await runReviewScience({ source, analysis, evidenceRef, target: process.env.QZ_JOB_BINARY, context });
    assert.equal(result.native.source_digest, `sha256:${canonicalJsonDigest(source)}`);
    assert.equal(result.native.method_version, 'qz.review-science.v1');
    assert.equal(result.native.scope, 'SHARED_ACCOUNT');
    assert.equal(result.native.observations, count);
    const delta = result.metrics.find(metric => metric.key === 'incremental.net_return_delta')!;
    assert.equal(delta.method, 'qz.review-science.v1'); assert.equal(delta.scope, 'SHARED_ACCOUNT');
    assert.equal(delta.evidenceRef, evidenceRef); assert.equal(delta.status, 'AVAILABLE');
    assert.ok(Math.abs(delta.value! + 0.003) < 1e-12);
    const lower = result.metrics.find(metric => metric.key === 'incremental.net_return_delta_lower')!;
    if (count === 128) { assert.equal(lower.status, 'AVAILABLE'); assert.ok(lower.value! < -0.0029); }
    else { assert.equal(lower.status, 'INSUFFICIENT'); assert.equal(lower.value, null); assert.equal(lower.reason, 'BOOTSTRAP_REQUIRES_30_COMPLETE_BLOCKS'); }
    assert.equal(JSON.stringify(source), frozen, 'the review producer must retain the original source');
  }
});
