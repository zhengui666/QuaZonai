import type { FastifyInstance } from 'fastify';
import { API } from '../../../packages/contracts/src/index.ts';
import type { createMaintenance } from './pm-maintenance.ts';

/** Register after owner authentication. Reads stay available during drain. */
export function registerMaintenanceRoutes(app: FastifyInstance, maintenance: ReturnType<typeof createMaintenance>) {
  app.addHook('onRequest', async request => {
    const path = request.url.split('?')[0]!;
    if (!path.startsWith(`${API}/`) || path.startsWith(`${API}/maintenance`) || path.startsWith(`${API}/session`) || path.startsWith(`${API}/devices`)) return;
    const reconciliation = request.method === 'POST' && /^\/api\/v3\/observations\/[^/]+\/refresh$/.test(path)
      || request.method === 'GET' && /^\/api\/v3\/observations\/[^/]+\/capital-exits$/.test(path);
    const business = !['GET', 'HEAD', 'OPTIONS'].includes(request.method) || /\/capacity$/.test(path) || reconciliation;
    if (business) await maintenance.beginRequest(request.id, reconciliation);
  });
  app.addHook('onResponse', async request => { maintenance.endRequest(request.id); });
  app.addHook('onError', async request => { maintenance.endRequest(request.id); });
  app.get(`${API}/maintenance`, () => maintenance.status());
  app.post(`${API}/maintenance/enter`, async (request, reply) => reply.code(202).send(await maintenance.enter(request.body)));
  app.post(`${API}/maintenance/resume`, async (request, reply) => reply.code(202).send(await maintenance.resume(request.body)));
}
