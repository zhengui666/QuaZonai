// Test-only, allowlisted evidence. Never retain URLs, DOM text, request data or errors.
const phases = ['start', 'setup-ready', 'editor-ready', 'submitted', 'unknown-visible',
  'cpu-disabled', 'snapshot-start', 'retry-name-start', 'retry-name-ready', 'retry-start',
  'retry-end', 'assertions-complete', 'finished'] as const;
type Phase = typeof phases[number];
const events = ['main-navigation', 'page-error', 'render-error', 'module-failed', 'api-failed',
  'get-auth-status', 'get-auth-session', 'get-projects', 'get-input-sets', 'get-input-set',
  'get-revision', 'get-source', 'get-runtime', 'get-readiness', 'get-runs', 'post-validate',
  'diagnostic-error'] as const;
type Event = typeof events[number];
const countFields = ['dialogs', 'visibleDialogs', 'retryButtons', 'visibleRetryButtons', 'roleMatchedRetryButtons'] as const;
const flagFields = ['dialogAriaHidden', 'retryDisabled', 'retryBusy', 'cpuDisabled', 'online',
  'loginVisible', 'fallbackVisible', 'unknownWarningVisible', 'loadingIconPresent',
  'loadingIconAriaHidden', 'retryHasAriaLabel'] as const;
const maxCount = 999;
const maxElapsed = 300_000;
const bounded = (value: number, max: number) => Math.min(max, Math.max(0, Math.floor(value)));

export function requestEvent(method: string, url: string): Event | undefined {
  let path: string;
  try { path = new URL(url).pathname; } catch { return undefined; }
  if (method === 'POST') return path === '/api/v2/data/validate' ? 'post-validate' : undefined;
  if (method !== 'GET') return undefined;
  const exact: Record<string, Event> = {
    '/api/v2/auth/status': 'get-auth-status', '/api/v2/auth/session': 'get-auth-session',
    '/api/v2/projects': 'get-projects', '/api/v2/input-sets': 'get-input-sets', '/api/v2/runs': 'get-runs',
  };
  if (Object.hasOwn(exact, path)) return exact[path];
  if (/^\/api\/v2\/input-sets\/[^/]+$/.test(path)) return 'get-input-set';
  if (/^\/api\/v2\/data\/revisions\/[^/]+$/.test(path)) return 'get-revision';
  if (/^\/api\/v2\/data\/sources\/[^/]+$/.test(path)) return 'get-source';
  if (/^\/api\/v2\/integrations\/runtimes\/[^/]+$/.test(path)) return 'get-runtime';
  if (/^\/api\/v2\/integrations\/runtimes\/[^/]+\/readiness$/.test(path)) return 'get-readiness';
  return undefined;
}

export function failureEvent(url: string): Event | undefined {
  let path: string;
  try { path = new URL(url).pathname; } catch { return undefined; }
  if (path.startsWith('/api/')) return 'api-failed';
  if (/\.(?:[cm]?js|tsx?)$/.test(path)) return 'module-failed';
  return undefined;
}

export function createRetryDiagnostics(now: () => number) {
  const started = now();
  const counts = Object.fromEntries(events.map(event => [event, 0])) as Record<Event, number>;
  const checkpoints: { phase: Phase; elapsedMs: number }[] = [];
  let snapshot: Record<string, boolean | number | null> | null = null;
  let elapsedMs = 0;
  function count(event: Event | undefined) {
    if (event && Object.hasOwn(counts, event)) counts[event] = Math.min(maxCount, counts[event] + 1);
  }
  return {
    count,
    mark(phase: Phase) {
      if (!phases.includes(phase) || checkpoints.some(entry => entry.phase === phase)) return;
      const duration = now() - started;
      if (Number.isFinite(duration)) elapsedMs = Math.max(elapsedMs, bounded(duration, maxElapsed));
      else count('diagnostic-error');
      checkpoints.push({ phase, elapsedMs });
    },
    capture(value: unknown) {
      const source = value && typeof value === 'object' ? value as Record<string, unknown> : {};
      const result: Record<string, boolean | number | null> = {};
      for (const field of countFields) {
        const item = source[field];
        result[field] = typeof item === 'number' && Number.isFinite(item) ? bounded(item, maxCount) : null;
      }
      for (const field of flagFields) result[field] = typeof source[field] === 'boolean' ? source[field] : null;
      snapshot = result;
    },
    complete() { return counts['diagnostic-error'] === 0 && snapshot !== null; },
    json(assertionsCompleted: boolean) {
      return JSON.stringify({ schema_version: 1, synthetic: true, assertionsCompleted,
        checkpoints, counts, beforeRetry: snapshot });
    },
  };
}
