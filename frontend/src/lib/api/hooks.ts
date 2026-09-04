import { useMutation, useQueries, useQuery, useQueryClient } from '@tanstack/react-query';
import { useEffect, useRef, useState } from 'react';
import { ApiError, apiRequest, jsonBody, normalizeList } from './client';
import type {
  AlphaQualification,
  ApprovalSnapshot,
  ConfigurationDataSource,
  ConfigurationCapitalContext,
  ConfigurationDataset,
  ConfigurationDownstream,
  ConfigurationEvaluationDatasetSelection,
  ConfigurationEvaluationDesignVersion,
  ConfigurationMandate,
  ConfigurationOperation,
  ConfigurationPromotionPolicyVersion,
  ConfigurationUniverse,
  HandoffOffer,
  PluginRelease,
  PortfolioCandidate,
  PortfolioProgram,
  Readiness,
  ResearchMission,
  ResearchProgram,
  RuntimeConfiguration,
  RuntimeConfigurationUpdate,
  SystemHealth,
  UUID,
  CodexChatgptAuthStatus,
  CodexChatgptDeviceLoginPoll,
  CodexChatgptDeviceLoginStart,
} from './types';

const keys = {
  readiness: ['readiness'] as const,
  health: ['health'] as const,
  runtimeConfiguration: ['runtime-configuration'] as const,
  codexAuth: ['codex-auth'] as const,
  programs: ['programs'] as const,
  program: (id: UUID) => ['program', id] as const,
  missionGraph: (id: UUID) => ['mission-graph', id] as const,
  alphas: ['alphas'] as const,
  alpha: (id: UUID) => ['alpha', id] as const,
  mandates: ['mandates'] as const,
  capitalContexts: ['capital-contexts'] as const,
  evaluationDatasetSelections: ['evaluation-dataset-selections'] as const,
  evaluationDesignVersions: ['evaluation-design-versions'] as const,
  promotionPolicyVersions: ['promotion-policy-versions'] as const,
  portfolioPrograms: ['portfolio-programs'] as const,
  candidate: (id: UUID) => ['candidate', id] as const,
  approvals: ['approvals'] as const,
  handoffs: ['handoffs'] as const,
  universes: ['universes'] as const,
  datasets: ['datasets'] as const,
  dataSources: ['data-sources'] as const,
  downstreams: ['downstreams'] as const,
  plugins: ['plugins'] as const,
  configurationOperation: (id: UUID) => ['configuration', 'operation', id] as const,
};

export const useReadiness = () => useQuery({ queryKey: keys.readiness, queryFn: () => apiRequest<Readiness>('/api/v1/readiness'), refetchInterval: 15_000 });
export const useHealth = () => useQuery({ queryKey: keys.health, queryFn: () => apiRequest<SystemHealth>('/api/v1/system/health'), refetchInterval: 15_000 });
export const useRuntimeConfiguration = () => useQuery({ queryKey: keys.runtimeConfiguration, queryFn: () => apiRequest<RuntimeConfiguration>('/api/v1/system/runtime-configuration') });
export function useCodexChatgptAuth() {
  const client = useQueryClient();
  const [deviceLogin, setDeviceLogin] = useState<CodexChatgptDeviceLoginStart | null>(null);
  const [pollResult, setPollResult] = useState<CodexChatgptDeviceLoginPoll | null>(null);
  const auth = useQuery({ queryKey: keys.codexAuth, queryFn: () => apiRequest<CodexChatgptAuthStatus>('/api/v1/system/codex-auth'), refetchInterval: 15_000 });
  const invalidateAuth = () => Promise.all([
    client.invalidateQueries({ queryKey: keys.codexAuth }),
    client.invalidateQueries({ queryKey: keys.runtimeConfiguration }),
    client.invalidateQueries({ queryKey: keys.health }),
    client.invalidateQueries({ queryKey: keys.readiness }),
  ]);
  const start = useMutation({
    mutationFn: () => apiRequest<CodexChatgptDeviceLoginStart>('/api/v1/system/codex-auth/chatgpt/device/start', { method: 'POST', body: jsonBody({}), idempotent: true }),
    onMutate: () => setPollResult(null),
    onSuccess: (result) => setDeviceLogin(result),
  });
  const poll = useMutation({
    mutationFn: (loginId: UUID) => apiRequest<CodexChatgptDeviceLoginPoll>(`/api/v1/system/codex-auth/chatgpt/device/${loginId}/poll`, { method: 'POST', body: jsonBody({}) }),
    onSuccess: async (result) => {
      if (result.status === 'PENDING' && deviceLogin) {
        setDeviceLogin({ ...deviceLogin, expires_at: result.expires_at ?? deviceLogin.expires_at, poll_after_seconds: result.poll_after_seconds ?? deviceLogin.poll_after_seconds });
      } else {
        setPollResult(result.status === 'FAILED' || result.status === 'EXPIRED' ? result : null);
        setDeviceLogin(null);
        await invalidateAuth();
      }
    },
    onError: async (error) => {
      const retryable = error instanceof ApiError && (
        [0, 408, 429].includes(error.status) || error.status >= 500
      );
      if (retryable) {
        setDeviceLogin((current) => current ? { ...current, poll_after_seconds: Math.min(60, Math.max(5, current.poll_after_seconds + 5)) } : current);
      } else {
        setPollResult(null);
        setDeviceLogin(null);
        await invalidateAuth();
      }
    },
  });
  const pollMutate = useRef(poll.mutate);
  pollMutate.current = poll.mutate;
  const cancel = useMutation({
    mutationFn: (loginId: UUID) => apiRequest<CodexChatgptDeviceLoginPoll>(`/api/v1/system/codex-auth/chatgpt/device/${loginId}`, { method: 'DELETE' }),
    onSuccess: async () => { setPollResult(null); setDeviceLogin(null); await invalidateAuth(); },
  });
  const disconnect = useMutation({
    mutationFn: () => apiRequest<CodexChatgptAuthStatus>('/api/v1/system/codex-auth/chatgpt', { method: 'DELETE' }),
    onSuccess: async () => { setPollResult(null); setDeviceLogin(null); await invalidateAuth(); },
  });
  useEffect(() => {
    if (!deviceLogin || document.visibilityState === 'hidden') return undefined;
    const timer = window.setTimeout(() => pollMutate.current(deviceLogin.login_id), deviceLogin.poll_after_seconds * 1000);
    return () => window.clearTimeout(timer);
  }, [deviceLogin]);
  useEffect(() => {
    const resume = () => { if (document.visibilityState === 'visible') setDeviceLogin((current) => current ? { ...current } : current); };
    document.addEventListener('visibilitychange', resume);
    return () => document.removeEventListener('visibilitychange', resume);
  }, []);
  return { auth, deviceLogin, pollResult, start, poll, cancel, disconnect };
}
export const usePrograms = () => useQuery({ queryKey: keys.programs, queryFn: async () => normalizeList(await apiRequest<ResearchProgram[] | { items: ResearchProgram[] }>('/api/v1/research-programs')) });
export const useProgram = (id?: UUID) => useQuery({ queryKey: id ? keys.program(id) : ['program', 'none'], queryFn: () => apiRequest<ResearchProgram>(`/api/v1/research-programs/${id}`), enabled: Boolean(id) });

async function fetchProgramMissions(id: UUID): Promise<ResearchMission[]> {
  const graph = await apiRequest<{ nodes?: ResearchMission[] }>(`/api/v1/research-programs/${id}/mission-graph`);
  if (!Array.isArray(graph.nodes)) {
    throw new ApiError(
      { kind: 'contract', message: 'Expected a mission graph response.' },
      0,
      'CONTRACT_MISMATCH',
    );
  }
  return graph.nodes;
}

export const useProgramMissions = (id?: UUID) => useQuery({ queryKey: id ? keys.missionGraph(id) : ['mission-graph', 'none'], queryFn: () => fetchProgramMissions(id as UUID), enabled: Boolean(id) });

export function useProgramMissionMatrix(ids: UUID[]) {
  return useQueries({
    queries: ids.map((id) => ({
      queryKey: keys.missionGraph(id),
      queryFn: () => fetchProgramMissions(id),
      staleTime: 5_000,
    })),
  });
}

export const useAlphaLibrary = () => useQuery({ queryKey: keys.alphas, queryFn: async () => normalizeList(await apiRequest<AlphaQualification[] | { items: AlphaQualification[] }>('/api/v1/alpha-library')) });
export const useAlpha = (id?: UUID) => useQuery({ queryKey: id ? keys.alpha(id) : ['alpha', 'none'], queryFn: () => apiRequest<AlphaQualification>(`/api/v1/alpha-library/${id}`), enabled: Boolean(id) });
export const useMandates = () => useQuery({ queryKey: keys.mandates, queryFn: async () => normalizeList(await apiRequest<{ items: ConfigurationMandate[] }>('/api/v1/portfolio-mandates')) });
export const usePortfolioPrograms = () => useQuery({ queryKey: keys.portfolioPrograms, queryFn: async () => normalizeList(await apiRequest<PortfolioProgram[] | { items: PortfolioProgram[] }>('/api/v1/portfolio-programs')) });
export const useCandidate = (id?: UUID) => useQuery({ queryKey: id ? keys.candidate(id) : ['candidate', 'none'], queryFn: () => apiRequest<PortfolioCandidate>(`/api/v1/portfolio-candidates/${id}`), enabled: Boolean(id) });

export function useCandidates(ids: UUID[]) {
  return useQueries({
    queries: ids.map((id) => ({ queryKey: keys.candidate(id), queryFn: () => apiRequest<PortfolioCandidate>(`/api/v1/portfolio-candidates/${id}`), staleTime: 5_000 })),
  });
}

export const useApprovals = () => useQuery({ queryKey: keys.approvals, queryFn: async () => normalizeList(await apiRequest<ApprovalSnapshot[] | { items: ApprovalSnapshot[] }>('/api/v1/approvals')), refetchInterval: 10_000 });
export const useHandoffs = () => useQuery({ queryKey: keys.handoffs, queryFn: async () => normalizeList(await apiRequest<HandoffOffer[] | { items: HandoffOffer[] }>('/api/v1/handoffs')), refetchInterval: 10_000 });
export const useDownstreams = () => useQuery({ queryKey: keys.downstreams, queryFn: async () => normalizeList(await apiRequest<{ items: ConfigurationDownstream[] }>('/api/v1/downstream-systems')) });

export const useConfigurationUniverses = () => useQuery({ queryKey: keys.universes, queryFn: async () => normalizeList(await apiRequest<{ items: ConfigurationUniverse[] }>('/api/v1/universes')) });
export const useConfigurationDataSources = () => useQuery({ queryKey: keys.dataSources, queryFn: async () => normalizeList(await apiRequest<{ items: ConfigurationDataSource[] }>('/api/v1/data-sources')) });
export const useConfigurationDatasets = () => useQuery({ queryKey: keys.datasets, queryFn: async () => normalizeList(await apiRequest<{ items: ConfigurationDataset[] }>('/api/v1/datasets')) });
export const useConfigurationMandates = () => useQuery({ queryKey: keys.mandates, queryFn: async () => normalizeList(await apiRequest<{ items: ConfigurationMandate[] }>('/api/v1/portfolio-mandates')) });
export const useConfigurationCapitalContexts = () => useQuery({ queryKey: keys.capitalContexts, queryFn: async () => normalizeList(await apiRequest<{ items: ConfigurationCapitalContext[] }>('/api/v1/capital-contexts')) });
export const useConfigurationEvaluationDatasetSelections = () => useQuery({ queryKey: keys.evaluationDatasetSelections, queryFn: async () => normalizeList(await apiRequest<{ items: ConfigurationEvaluationDatasetSelection[] }>('/api/v1/evaluation-dataset-selections')) });
export const useConfigurationEvaluationDesignVersions = () => useQuery({ queryKey: keys.evaluationDesignVersions, queryFn: async () => normalizeList(await apiRequest<{ items: ConfigurationEvaluationDesignVersion[] }>('/api/v1/evaluation-design-versions')) });
export const useConfigurationPromotionPolicyVersions = () => useQuery({ queryKey: keys.promotionPolicyVersions, queryFn: async () => normalizeList(await apiRequest<{ items: ConfigurationPromotionPolicyVersion[] }>('/api/v1/promotion-policy-versions')) });
export const useConfigurationDownstreams = () => useQuery({ queryKey: keys.downstreams, queryFn: async () => normalizeList(await apiRequest<{ items: ConfigurationDownstream[] }>('/api/v1/downstream-systems')) });
export const useConfigurationOperation = (id?: UUID) => useQuery({ queryKey: id ? keys.configurationOperation(id) : ['configuration', 'operation', 'none'], queryFn: () => apiRequest<ConfigurationOperation>(`/api/v1/operations/${id}`), enabled: Boolean(id), refetchInterval: 5_000 });

export function usePluginReleases() {
  return useQuery({
    queryKey: keys.plugins,
    queryFn: async () => {
      try { return normalizeList(await apiRequest<PluginRelease[] | { items: PluginRelease[] }>('/api/v1/plugin-releases')); }
      catch (error) { if ((error as { status?: number }).status === 404) return []; throw error; }
    },
  });
}

export function useProgramAction(id: UUID, action: 'pause' | 'resume' | 'archive', expectedRevision?: number) {
  const client = useQueryClient();
  return useMutation({ mutationFn: (reason?: string) => apiRequest(`/api/v1/research-programs/${id}/${action}`, { method: 'POST', body: jsonBody({ ...(reason ? { reason } : {}), ...(expectedRevision === undefined ? {} : { expected_revision: expectedRevision }) }), idempotent: true }), onSuccess: async () => { await Promise.all([client.invalidateQueries({ queryKey: keys.program(id) }), client.invalidateQueries({ queryKey: keys.programs })]); } });
}

export function useApprovalDecision(id: UUID) {
  const client = useQueryClient();
  return {
    approve: useMutation({ mutationFn: (downstream_system_id: UUID | null) => apiRequest(`/api/v1/approvals/${id}/approve`, { method: 'POST', body: jsonBody({ ...(downstream_system_id ? { downstream_system_id } : {}), expected_state: 'PENDING' }), idempotent: true }), onSuccess: async () => { await Promise.all([client.invalidateQueries({ queryKey: keys.approvals }), client.invalidateQueries({ queryKey: keys.handoffs })]); } }),
    reject: useMutation({ mutationFn: (payload: { reason_code: string; note?: string }) => apiRequest(`/api/v1/approvals/${id}/reject`, { method: 'POST', body: jsonBody({ ...payload, expected_state: 'PENDING' }), idempotent: true }), onSuccess: () => client.invalidateQueries({ queryKey: keys.approvals }) }),
  };
}

export function useRevokeHandoff(id: UUID) {
  const client = useQueryClient();
  return useMutation({ mutationFn: (reason: string) => apiRequest(`/api/v1/handoffs/${id}/revoke`, { method: 'POST', body: jsonBody({ reason_code: reason }), idempotent: true }), onSuccess: () => client.invalidateQueries({ queryKey: keys.handoffs }) });
}

export function useCreateConfigurationUniverse() {
  const client = useQueryClient();
  return useMutation({ mutationFn: (payload: Record<string, unknown>) => apiRequest<ConfigurationUniverse>('/api/v1/universes', { method: 'POST', body: jsonBody(payload), idempotent: true }), onSuccess: () => client.invalidateQueries({ queryKey: keys.universes }) });
}

export function useCreateConfigurationUniverseVersion() {
  const client = useQueryClient();
  return useMutation({ mutationFn: ({ universeId, payload }: { universeId: UUID; payload: Record<string, unknown> }) => apiRequest<ConfigurationUniverse>(`/api/v1/universes/${universeId}/versions`, { method: 'POST', body: jsonBody(payload), idempotent: true }), onSuccess: () => client.invalidateQueries({ queryKey: keys.universes }) });
}

export function useCreateConfigurationDataSource() {
  const client = useQueryClient();
  return useMutation({ mutationFn: (payload: Record<string, unknown>) => apiRequest<ConfigurationDataSource>('/api/v1/data-sources', { method: 'POST', body: jsonBody(payload), idempotent: true }), onSuccess: () => client.invalidateQueries({ queryKey: keys.dataSources }) });
}

export function useRequestConfigurationDataSourcePreflight() {
  const client = useQueryClient();
  return useMutation({ mutationFn: (dataSourceId: UUID) => apiRequest<ConfigurationOperation>(`/api/v1/data-sources/${dataSourceId}/preflight`, { method: 'POST', body: jsonBody({}), idempotent: true }), onSuccess: () => client.invalidateQueries({ queryKey: keys.dataSources }) });
}

export function useRequestConfigurationDatasetMaterialization() {
  const client = useQueryClient();
  return useMutation({ mutationFn: (payload: Record<string, unknown>) => apiRequest<ConfigurationOperation>('/api/v1/datasets/materializations', { method: 'POST', body: jsonBody(payload), idempotent: true }), onSuccess: () => client.invalidateQueries({ queryKey: keys.datasets }) });
}

export function useCreateConfigurationMandate() {
  const client = useQueryClient();
  return useMutation({ mutationFn: (payload: Record<string, unknown>) => apiRequest<ConfigurationMandate>('/api/v1/portfolio-mandates', { method: 'POST', body: jsonBody(payload), idempotent: true }), onSuccess: () => client.invalidateQueries({ queryKey: keys.mandates }) });
}

export function useCreateConfigurationMandateVersion() {
  const client = useQueryClient();
  return useMutation({ mutationFn: ({ mandateId, payload }: { mandateId: UUID; payload: Record<string, unknown> }) => apiRequest<ConfigurationMandate>(`/api/v1/portfolio-mandates/${mandateId}/versions`, { method: 'POST', body: jsonBody(payload), idempotent: true }), onSuccess: () => client.invalidateQueries({ queryKey: keys.mandates }) });
}

export function useCreateConfigurationCapitalContext() {
  const client = useQueryClient();
  return useMutation({ mutationFn: (payload: Record<string, unknown>) => apiRequest<ConfigurationCapitalContext>('/api/v1/capital-contexts', { method: 'POST', body: jsonBody(payload), idempotent: true }), onSuccess: () => client.invalidateQueries({ queryKey: keys.capitalContexts }) });
}

export function useCreateConfigurationEvaluationDatasetSelection() {
  const client = useQueryClient();
  return useMutation({ mutationFn: (payload: Record<string, unknown>) => apiRequest<ConfigurationEvaluationDatasetSelection>('/api/v1/evaluation-dataset-selections', { method: 'POST', body: jsonBody(payload), idempotent: true }), onSuccess: () => client.invalidateQueries({ queryKey: keys.evaluationDatasetSelections }) });
}

export function useCreateConfigurationEvaluationDesignVersion() {
  const client = useQueryClient();
  return useMutation({ mutationFn: (payload: Record<string, unknown>) => apiRequest<ConfigurationEvaluationDesignVersion>('/api/v1/evaluation-design-versions', { method: 'POST', body: jsonBody(payload), idempotent: true }), onSuccess: () => client.invalidateQueries({ queryKey: keys.evaluationDesignVersions }) });
}

export function useCreateConfigurationPromotionPolicyVersion() {
  const client = useQueryClient();
  return useMutation({ mutationFn: (payload: Record<string, unknown>) => apiRequest<ConfigurationPromotionPolicyVersion>('/api/v1/promotion-policy-versions', { method: 'POST', body: jsonBody(payload), idempotent: true }), onSuccess: () => client.invalidateQueries({ queryKey: keys.promotionPolicyVersions }) });
}

export function useUpdateRuntimeConfiguration() {
  const client = useQueryClient();
  const pendingSave = useRef<{ body: string; key: string } | null>(null);
  return useMutation({
    mutationFn: (payload: RuntimeConfigurationUpdate) => {
      const body = jsonBody(payload);
      let pending = pendingSave.current;
      if (pending === null || pending.body !== body) {
        pending = { body, key: crypto.randomUUID() };
        pendingSave.current = pending;
      }
      return apiRequest<RuntimeConfiguration>('/api/v1/system/runtime-configuration', {
        method: 'PUT',
        body,
        idempotent: true,
        headers: { 'Idempotency-Key': pending.key },
      });
    },
    onSuccess: async () => {
      pendingSave.current = null;
      await Promise.all([
        client.invalidateQueries({ queryKey: keys.runtimeConfiguration }),
        client.invalidateQueries({ queryKey: keys.health }),
        client.invalidateQueries({ queryKey: keys.readiness }),
      ]);
    },
  });
}
