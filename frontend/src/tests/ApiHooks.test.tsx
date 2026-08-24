import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import type { ReactNode } from 'react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { useApprovalDecision, useCreateDataSource, useUpdateRuntimeConfiguration } from '../lib/api/hooks';
import { jsonResponse } from './testUtils';

function createWrapper(client: QueryClient) {
  return function Wrapper({ children }: { children: ReactNode }) {
    return <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  };
}

afterEach(() => {
  vi.restoreAllMocks();
});

describe('API hooks', () => {
  it('creates a data source and invalidates dependent server state', async () => {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
    const invalidate = vi.spyOn(client, 'invalidateQueries');
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation(() => jsonResponse({ id: 'source-1', name: 'PIT Data', state: 'ACTIVE' }));
    const { result } = renderHook(() => useCreateDataSource(), { wrapper: createWrapper(client) });

    await act(async () => {
      await result.current.mutateAsync({ name: 'PIT Data', provider: 'Approved', fields: ['event_time', 'available_time'], state: 'STAGED' });
    });

    expect(fetchMock).toHaveBeenCalledWith('/api/v1/data-sources', expect.objectContaining({ method: 'POST' }));
    const options = fetchMock.mock.calls[0][1] as RequestInit;
    expect(JSON.parse(String(options.body))).toMatchObject({ name: 'PIT Data', provider: 'Approved', state: 'STAGED' });
    await waitFor(() => {
      expect(invalidate).toHaveBeenCalledWith({ queryKey: ['data-sources'] });
      expect(invalidate).toHaveBeenCalledWith({ queryKey: ['readiness'] });
      expect(invalidate).toHaveBeenCalledWith({ queryKey: ['health'] });
    });
  });

  it('approves one immutable candidate and refreshes approvals plus handoffs', async () => {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
    const invalidate = vi.spyOn(client, 'invalidateQueries');
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation(() => jsonResponse({ id: 'approval-1', state: 'APPROVED' }));
    const { result } = renderHook(() => useApprovalDecision('approval-1'), { wrapper: createWrapper(client) });

    await act(async () => {
      await result.current.approve.mutateAsync('paper-downstream');
    });

    expect(fetchMock).toHaveBeenCalledWith('/api/v1/approvals/approval-1/approve', expect.objectContaining({ method: 'POST' }));
    const options = fetchMock.mock.calls[0][1] as RequestInit;
    expect(JSON.parse(String(options.body))).toEqual({ downstream_system_id: 'paper-downstream', expected_state: 'PENDING' });
    await waitFor(() => {
      expect(invalidate).toHaveBeenCalledWith({ queryKey: ['approvals'] });
      expect(invalidate).toHaveBeenCalledWith({ queryKey: ['handoffs'] });
    });
  });

  it('updates Codex and worker runtime configuration with revision and idempotency', async () => {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
    const invalidate = vi.spyOn(client, 'invalidateQueries');
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation(() => jsonResponse({ revision: 8, codex_model: 'gpt-5.6-sol', codex_base_url: 'https://gateway.example/v1', codex_api_key_configured: true }));
    const { result } = renderHook(() => useUpdateRuntimeConfiguration(), { wrapper: createWrapper(client) });
    const payload = {
      expected_revision: 7,
      codex_model: 'gpt-5.6-sol',
      codex_base_url: 'https://gateway.example/v1',
      codex_api_key: 'secret-value',
      clear_codex_api_key: false,
      max_plugin_wheel_bytes: 268435456,
      plugin_validation_timeout_seconds: 180,
      bundle_build_timeout_seconds: 600,
      plugin_job_timeout_seconds: 900,
      mission_job_timeout_seconds: 1800,
      job_poll_seconds: 1,
      job_lease_seconds: 60,
    };

    await act(async () => {
      await result.current.mutateAsync(payload);
    });

    expect(fetchMock).toHaveBeenCalledWith('/api/v1/system/runtime-configuration', expect.objectContaining({ method: 'PUT' }));
    const options = fetchMock.mock.calls[0][1] as RequestInit;
    expect(JSON.parse(String(options.body))).toEqual(payload);
    const headers = options.headers as Headers;
    expect(headers.get('Idempotency-Key')).toBeTruthy();
    await waitFor(() => {
      expect(invalidate).toHaveBeenCalledWith({ queryKey: ['runtime-configuration'] });
      expect(invalidate).toHaveBeenCalledWith({ queryKey: ['health'] });
      expect(invalidate).toHaveBeenCalledWith({ queryKey: ['readiness'] });
    });
  });
});
