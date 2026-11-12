import assert from 'node:assert/strict';
import { test } from 'node:test';
import { setup } from './support.ts';

test('PM retained resource reads and account commands stay authenticated with typed missing identities', async () => {
  const state = await setup(); const missing = encodeURIComponent('absent:original-resource');
  try {
    for (const path of ['/capabilities', `/mandates/${missing}/decision`, `/mandates/${missing}/decisions`, `/cycles/${missing}`,
      `/capacity-estimates/${missing}`, `/pm-evidence/${missing}`, `/campaigns/${missing}`, `/campaigns/${missing}/trials`, `/campaigns/${missing}/decision`,
      `/strategies/${missing}/versions`, `/strategies/${missing}/assessments`]) {
      assert.equal((await state.service.app.inject({ url: `/api/v3${path}` })).statusCode, 401);
      const response = await state.service.app.inject({ url: `/api/v3${path}`, headers: state.headers });
      assert.equal(response.statusCode, path === '/capabilities' ? 200 : 404, `${path}: ${response.body}`);
      if (path !== '/capabilities') assert.match(response.json().error, /not_found$/);
    }
    for (const path of [`/campaigns/${missing}/pause`, `/strategies/${missing}/pause`, `/strategies/${missing}/resume`,
      `/mandates/${missing}/targets`, `/mandates/${missing}/retire`, `/mandates/${missing}/refresh-data`]) {
      const payload = { requestId: 'original-command', expectedRevision: '0', decisionId: 'absent:decision' };
      assert.equal((await state.service.app.inject({ method: 'POST', url: `/api/v3${path}`, payload })).statusCode, 401);
      const response = await state.service.app.inject({ method: 'POST', url: `/api/v3${path}`, payload, headers: state.headers });
      assert.equal(response.statusCode, 404, `${path}: ${response.body}`);
      assert.match(response.json().error, /not_found$/);
    }
    for (const path of ['/mandates', '/campaigns', `/mandates/${missing}/intent-clarifications`, `/campaigns/${missing}/close`, `/campaigns/${missing}/resume`]) {
      const payload = { requestId: 'removed-custom-research' };
      assert.equal((await state.service.app.inject({ method: 'POST', url: `/api/v3${path}`, payload })).statusCode, 401);
      const response = await state.service.app.inject({ method: 'POST', url: `/api/v3${path}`, payload, headers: state.headers });
      assert.equal(response.statusCode, 410, `${path}: ${response.body}`);
      assert.equal(response.json().error, 'pm_custom_research_removed');
    }
    assert.deepEqual((await state.service.app.inject({ url: '/api/v3/mandates', headers: state.headers })).json(), []);
  } finally { await state.cleanup(); }
});
