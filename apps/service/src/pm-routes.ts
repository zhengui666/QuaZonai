import type { FastifyInstance } from 'fastify';
import { API } from '../../../packages/contracts/src/index.ts';
import type { createPmPlatform } from './pm-platform.ts';

export function registerPmRoutes(app: FastifyInstance, work: ReturnType<typeof createPmPlatform>['work']) {
  app.get(`${API}/capabilities`, () => work.readResource('capabilities'));
  for (const [path, kind] of [
    ['mandates/:id/decision', 'decision'], ['mandates/:id/decisions', 'decisions'],
    ['cycles/:id', 'cycle'], ['capacity-estimates/:id', 'capacity'],
    ['pm-evidence/:id', 'evidence'],
    ['campaigns/:id', 'campaign'], ['campaigns/:id/trials', 'trials'], ['campaigns/:id/decision', 'campaignDecision'],
    ['strategies/:id/versions', 'strategyVersions'], ['strategies/:id/assessments', 'strategyAssessments'],
  ] as const) {
    app.get<{ Params: { id: string } }>(`${API}/${path}`, request => work.readResource(kind, request.params.id));
  }
  app.post(`${API}/campaigns`, async (request, reply) => reply.code(202).send(await work.startCampaign(request.body)));
  for (const action of ['close', 'pause', 'resume'] as const) {
    app.post<{ Params: { id: string } }>(`${API}/campaigns/:id/${action}`,
      async (request, reply) => reply.code(202).send(await work.campaignCommand(request.params.id, action, request.body)));
  }
  for (const action of ['pause', 'resume'] as const) {
    app.post<{ Params: { id: string } }>(`${API}/strategies/:id/${action}`,
      async (request, reply) => reply.code(202).send(await work.strategyCommand(request.params.id, action, request.body)));
  }
  app.get(`${API}/mandates`, () => work.list());
  app.post(`${API}/mandates`, async (request, reply) => reply.code(202).send(await work.start(request.body)));
  app.get<{ Params: { id: string } }>(`${API}/mandates/:id`, request => work.get(request.params.id));
  app.get<{ Params: { id: string } }>(`${API}/mandates/:id/events`, request => work.events(request.params.id));
  app.post<{ Params: { id: string } }>(`${API}/mandates/:id/targets`, async (request, reply) => reply.code(202).send(await work.offerTarget(request.params.id, request.body)));
  for (const [suffix, action] of [['recompute', work.recompute], ['retire', work.retire], ['refresh-data', work.refreshData]] as const) {
    app.post<{ Params: { id: string } }>(`${API}/mandates/:id/${suffix}`, async (request, reply) => reply.code(202).send(await action(request.params.id, request.body)));
  }
}
