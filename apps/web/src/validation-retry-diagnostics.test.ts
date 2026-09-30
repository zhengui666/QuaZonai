import { describe, expect, it } from 'vitest';
import { createRetryDiagnostics, failureEvent, requestEvent } from '../tests/validation-retry-diagnostics';

describe('synthetic validation retry diagnostics', () => {
  it('classifies only known methods/endpoints without retaining URL contents', () => {
    expect(requestEvent('GET', 'https://private:secret@fixture.invalid/api/v2/integrations/runtimes/private-id/readiness?token=secret')).toBe('get-readiness');
    expect(requestEvent('POST', 'https://fixture.invalid/api/v2/data/validate')).toBe('post-validate');
    expect(requestEvent('POST', 'https://fixture.invalid/api/v2/auth/login')).toBeUndefined();
    expect(requestEvent('GET', 'https://fixture.invalid/api/private')).toBeUndefined();
    expect(requestEvent('GET', 'invalid')).toBeUndefined();
    expect(failureEvent('https://fixture.invalid/src/private.tsx?secret')).toBe('module-failed');
    expect(failureEvent('https://fixture.invalid/api/private?secret')).toBe('api-failed');
    expect(failureEvent('https://fixture.invalid/private.png')).toBeUndefined();
  });

  it('copies only bounded counts and boolean snapshot fields', () => {
    const recorder = createRetryDiagnostics(() => 0);
    recorder.capture({ dialogs: 2.9, visibleDialogs: -5, retryButtons: 1e12, visibleRetryButtons: NaN,
      roleMatchedRetryButtons: 0, loadingIconPresent: true, loadingIconAriaHidden: false, retryHasAriaLabel: false,
      online: true, retryDisabled: 'secret', cpuDisabled: false, rawDOM: 'secret', headers: { secret: 'secret' } });
    const encoded = recorder.json(false);
    expect(encoded).not.toContain('secret'); expect(encoded).not.toContain('rawDOM');
    expect(JSON.parse(encoded).beforeRetry).toMatchObject({ dialogs: 2, visibleDialogs: 0,
      retryButtons: 999, visibleRetryButtons: null, roleMatchedRetryButtons: 0,
      loadingIconPresent: true, loadingIconAriaHidden: false, retryHasAriaLabel: false,
      online: true, retryDisabled: null, cpuDisabled: false });
  });

  it('bounds events/phases and ignores unrecognized strings', () => {
    let clock = 0;
    const recorder = createRetryDiagnostics(() => clock);
    recorder.mark('start'); clock = 1e12;
    for (let i = 0; i < 2000; i++) { recorder.count('main-navigation'); recorder.mark('retry-start'); }
    recorder.count('secret' as never); recorder.mark('secret' as never);
    const encoded = recorder.json(false); const value = JSON.parse(encoded);
    expect(value.counts['main-navigation']).toBe(999);
    expect(value.checkpoints).toEqual([{ phase: 'start', elapsedMs: 0 }, { phase: 'retry-start', elapsedMs: 300000 }]);
    expect(encoded).not.toContain('secret'); expect(encoded.length).toBeLessThan(4096);
  });

  it('keeps elapsed checkpoints monotonic and records diagnostic incompleteness', () => {
    let clock = 100;
    const recorder = createRetryDiagnostics(() => clock);
    expect(recorder.complete()).toBe(false);
    clock = 110; recorder.mark('start'); clock = 90; recorder.mark('setup-ready');
    clock = NaN; recorder.mark('editor-ready'); recorder.capture({ online: true });
    expect(JSON.parse(recorder.json(false)).checkpoints.map((item: { elapsedMs: number }) => item.elapsedMs)).toEqual([10, 10, 10]);
    expect(recorder.complete()).toBe(false);
    const valid = createRetryDiagnostics(() => 0); valid.capture({ online: true });
    expect(valid.complete()).toBe(true);
  });
});
