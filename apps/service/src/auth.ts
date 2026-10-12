import argon2 from 'argon2';
import { createHash, randomBytes, randomUUID, timingSafeEqual } from 'node:crypto';
import type { FastifyInstance, FastifyRequest } from 'fastify';
import type { ResearchHost } from '../../research/src/host.ts';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { API } from '../../../packages/contracts/src/index.ts';
import { Application, object, Problem, text } from './facts.ts';
import type { ServiceConfig } from './config.ts';

const cookie = 'qz_session';
const digest = (token: string) => createHash('sha512').update(token).digest('hex');
export function registerAuth(app: FastifyInstance, host: ResearchHost, config: ServiceConfig) {
  const facts = () => host.harness.snapshot(Application, BACKGROUND_CONTEXT);
  async function session(request: FastifyRequest) {
    const bearer = request.headers.authorization?.match(/^Bearer (.+)$/)?.[1];
    const token = bearer ?? request.cookies[cookie]; if (!token) return null;
    const id = token.split('.')[0]; const entry = (await facts())?.sessions[id];
    if (!entry || entry.expires <= Date.now() || entry.kind !== (bearer ? 'device' : 'browser')) return null;
    return timingSafeEqual(Buffer.from(entry.digest, 'hex'), Buffer.from(digest(token), 'hex')) ? id : null;
  }
  const publicRoutes = new Set([`${API}/session`, `${API}/session/setup`, `${API}/session/login`, `${API}/devices/login`]);
  app.addHook('onRequest', async (request) => {
    const path = request.url.split('?')[0];
    if (!path.startsWith(`${API}/`)) return;
    if (request.method !== 'GET' && request.headers.origin) {
      const expected = config.publicOrigin ?? `${request.protocol}://${request.headers.host}`;
      if (request.headers.origin !== expected) throw new Problem('origin_not_allowed', 403);
    }
    if (!publicRoutes.has(path) && !await session(request)) throw new Problem('authentication_required', 401);
  });
  async function verify(body: unknown) {
    const password = text(object(body).password, 'password');
    const hash = (await facts())?.passwordHash ?? config.passwordHash;
    if (!hash || !await argon2.verify(hash, password)) throw new Problem('invalid_password', 401);
  }
  async function issue(kind: string, name: string, expires: number) {
    const id = randomUUID(); const token = `${id}.${randomBytes(32).toString('base64url')}`;
    await host.harness.commit(async (tx) => {
      const state = await tx.doc(Application); state.sessions[id] = { digest: digest(token), expires, kind, name };
    }, BACKGROUND_CONTEXT);
    return { id, token };
  }
  app.get(`${API}/session`, async (request) => ({ setupRequired: !(await facts())?.passwordHash && !config.passwordHash, authenticated: !!await session(request) }));
  app.post(`${API}/session/setup`, async (request, reply) => {
    const password = text(object(request.body).password, 'password');
    const hash = await argon2.hash(password);
    await host.harness.commit(async (tx) => {
      const state = await tx.doc(Application);
      if (state.passwordHash || config.passwordHash) throw new Problem('owner_already_configured', 409);
      state.passwordHash = hash;
    }, BACKGROUND_CONTEXT);
    const { token } = await issue('browser', 'Owner', Date.now() + 86_400_000);
    reply.setCookie(cookie, token, { path: '/', httpOnly: true, sameSite: 'strict', secure: config.secureCookies });
    return { setupRequired: false, authenticated: true };
  });
  app.post(`${API}/session/login`, async (request, reply) => {
    await verify(request.body); const remember = object(request.body).remember === true;
    const seconds = remember ? 30 * 86_400 : 86_400;
    const { token } = await issue('browser', 'Owner', Date.now() + seconds * 1000);
    reply.setCookie(cookie, token, { path: '/', httpOnly: true, sameSite: 'strict', secure: config.secureCookies, ...(remember ? { maxAge: seconds } : {}) });
    return { setupRequired: false, authenticated: true };
  });
  app.delete(`${API}/session`, async (request, reply) => {
    const id = await session(request);
    if (id) await host.harness.commit(async (tx) => { delete (await tx.doc(Application)).sessions[id]; }, BACKGROUND_CONTEXT);
    reply.clearCookie(cookie, { path: '/' }); return { setupRequired: false, authenticated: false };
  });
  app.post(`${API}/devices/login`, async (request) => {
    await verify(request.body); const body = object(request.body);
    return issue('device', typeof body.name === 'string' ? body.name : 'CLI', Date.now() + 365 * 86_400_000);
  });
  app.delete(`${API}/devices/current`, async (request, reply) => {
    if (!request.headers.authorization) throw new Problem('device_token_required', 401);
    const id = await session(request);
    if (id) await host.harness.commit(async (tx) => { delete (await tx.doc(Application)).sessions[id]; }, BACKGROUND_CONTEXT);
    return reply.code(204).send();
  });
}
