import { useMutation, useQueries, useQuery, useQueryClient } from '@tanstack/react-query';
import { apiRequest, jsonBody, normalizeList } from './client';
import type {
  ActivityEvent,
  AlphaQualification,
  ApprovalSnapshot,
  DataSource,
  DatasetRevision,
  DownstreamSystem,
  HandoffOffer,
  IdeaPreview,
  MarketUniverse,
  PluginRelease,
  PortfolioCandidate,
  PortfolioMandate,
  PortfolioProgram,
  Readiness,
  ResearchMission,
  ResearchProgram,
  SystemHealth,
  UUID,
} from './types';

const keys = {
  readiness: ['readiness'] as const,
  health: ['health'] as const,
  programs: ['programs'] as const,
  program: (id: UUID) => ['program', id] as const,
  missions: (id: UUID) => ['missions', id] as const,
  activity: (id: UUID) => ['activity', id] as const,
  alphas: ['alphas'] as const,
  alpha: (id: UUID) => ['alpha', id] as const,
  mandates: ['mandates'] as const,
  portfolioPrograms: ['portfolio-programs'] as const,
  candidate: (id: UUID) => ['candidate', id] as const,
  approvals: ['approvals'] as const,
  handoffs: ['handoffs'] as const,
  universes: ['universes'] as const,
  datasets: ['datasets'] as const,
  dataSources: ['data-sources'] as const,
  downstreams: ['downstreams'] as const,
  plugins: ['plugins'] as const,
};

export const useReadiness = () => useQuery({ queryKey: keys.readiness, queryFn: () => apiRequest<Readiness>('/api/v1/readiness'), refetchInterval: 15_000 });
export const useHealth = () => useQuery({ queryKey: keys.health, queryFn: () => apiRequest<SystemHealth>('/api/v1/system/health'), refetchInterval: 15_000 });
export const usePrograms = () => useQuery({ queryKey: keys.programs, queryFn: async () => normalizeList(await apiRequest<ResearchProgram[] | { items: ResearchProgram[] }>('/api/v1/research-programs')) });
export const useProgram = (id?: UUID) => useQuery({ queryKey: id ? keys.program(id) : ['program', 'none'], queryFn: () => apiRequest<ResearchProgram>(`/api/v1/research-programs/${id}`), enabled: Boolean(id) });
export const useProgramMissions = (id?: UUID) => useQuery({ queryKey: id ? keys.missions(id) : ['missions', 'none'], queryFn: async () => normalizeList(await apiRequest<ResearchMission[] | { items: ResearchMission[] }>(`/api/v1/research-programs/${id}/missions`)), enabled: Boolean(id) });
export const useProgramActivity = (id?: UUID) => useQuery({ queryKey: id ? keys.activity(id) : ['activity', 'none'], queryFn: async () => normalizeList(await apiRequest<ActivityEvent[] | { items: ActivityEvent[] }>(`/api/v1/research-programs/${id}/activity`)), enabled: Boolean(id), refetchInterval: 8_000 });

export function useProgramMissionMatrix(ids: UUID[]) {
  return useQueries({
    queries: ids.map((id) => ({
      queryKey: keys.missions(id),
      queryFn: async () => normalizeList(await apiRequest<ResearchMission[] | { items: ResearchMission[] }>(`/api/v1/research-programs/${id}/missions`)),
      staleTime: 5_000,
    })),
  });
}

export const useAlphaLibrary = () => useQuery({ queryKey: keys.alphas, queryFn: async () => normalizeList(await apiRequest<AlphaQualification[] | { items: AlphaQualification[] }>('/api/v1/alpha-library')) });
export const useAlpha = (id?: UUID) => useQuery({ queryKey: id ? keys.alpha(id) : ['alpha', 'none'], queryFn: () => apiRequest<AlphaQualification>(`/api/v1/alpha-library/${id}`), enabled: Boolean(id) });
export const useMandates = () => useQuery({ queryKey: keys.mandates, queryFn: async () => normalizeList(await apiRequest<PortfolioMandate[] | { items: PortfolioMandate[] }>('/api/v1/portfolio-mandates')) });
export const usePortfolioPrograms = () => useQuery({ queryKey: keys.portfolioPrograms, queryFn: async () => normalizeList(await apiRequest<PortfolioProgram[] | { items: PortfolioProgram[] }>('/api/v1/portfolio-programs')) });
export const useCandidate = (id?: UUID) => useQuery({ queryKey: id ? keys.candidate(id) : ['candidate', 'none'], queryFn: () => apiRequest<PortfolioCandidate>(`/api/v1/portfolio-candidates/${id}`), enabled: Boolean(id) });
export const useApprovals = () => useQuery({ queryKey: keys.approvals, queryFn: async () => normalizeList(await apiRequest<ApprovalSnapshot[] | { items: ApprovalSnapshot[] }>('/api/v1/approvals')), refetchInterval: 10_000 });
export const useHandoffs = () => useQuery({ queryKey: keys.handoffs, queryFn: async () => normalizeList(await apiRequest<HandoffOffer[] | { items: HandoffOffer[] }>('/api/v1/handoffs')), refetchInterval: 10_000 });
export const useUniverses = () => useQuery({ queryKey: keys.universes, queryFn: async () => normalizeList(await apiRequest<MarketUniverse[] | { items: MarketUniverse[] }>('/api/v1/universes')) });
export const useDatasets = () => useQuery({ queryKey: keys.datasets, queryFn: async () => normalizeList(await apiRequest<DatasetRevision[] | { items: DatasetRevision[] }>('/api/v1/datasets')) });
export const useDataSources = () => useQuery({ queryKey: keys.dataSources, queryFn: async () => normalizeList(await apiRequest<DataSource[] | { items: DataSource[] }>('/api/v1/data-sources')) });
export const useDownstreams = () => useQuery({ queryKey: keys.downstreams, queryFn: async () => normalizeList(await apiRequest<DownstreamSystem[] | { items: DownstreamSystem[] }>('/api/v1/downstream-systems')) });

export function usePluginReleases() {
  return useQuery({
    queryKey: keys.plugins,
    queryFn: async () => {
      try {
        return normalizeList(await apiRequest<PluginRelease[] | { items: PluginRelease[] }>('/api/v1/plugin-releases'));
      } catch (error) {
        if ((error as { status?: number }).status === 404) return [];
        throw error;
      }
    },
  });
}

export const useIdeaPreview = () => useMutation({ mutationFn: (idea: string) => apiRequest<IdeaPreview>('/api/v1/ideas/preview', { method: 'POST', body: jsonBody({ idea }), idempotent: true }) });

export function useStartResearch() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (payload: { idea: string; answers?: Record<string, string>; overlap_action?: string }) => apiRequest<ResearchProgram>('/api/v1/research-programs', { method: 'POST', body: jsonBody(payload), idempotent: true }),
    onSuccess: () => client.invalidateQueries({ queryKey: keys.programs }),
  });
}

export function useProgramAction(id: UUID, action: 'pause' | 'resume' | 'archive' | 'restore') {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (reason?: string) => apiRequest(`/api/v1/research-programs/${id}/${action}`, { method: 'POST', body: jsonBody(reason ? { reason } : {}), idempotent: true }),
    onSuccess: async () => { await Promise.all([client.invalidateQueries({ queryKey: keys.program(id) }), client.invalidateQueries({ queryKey: keys.programs })]); },
  });
}

export function useApprovalDecision(id: UUID) {
  const client = useQueryClient();
  return {
    approve: useMutation({
      mutationFn: (downstream_system_id: UUID) => apiRequest(`/api/v1/approvals/${id}/approve`, { method: 'POST', body: jsonBody({ downstream_system_id, expected_state: 'PENDING' }), idempotent: true }),
      onSuccess: async () => { await Promise.all([client.invalidateQueries({ queryKey: keys.approvals }), client.invalidateQueries({ queryKey: keys.handoffs })]); },
    }),
    reject: useMutation({
      mutationFn: (payload: { reason_code: string; note?: string }) => apiRequest(`/api/v1/approvals/${id}/reject`, { method: 'POST', body: jsonBody({ ...payload, expected_state: 'PENDING' }), idempotent: true }),
      onSuccess: () => client.invalidateQueries({ queryKey: keys.approvals }),
    }),
  };
}

export function useRevokeHandoff(id: UUID) {
  const client = useQueryClient();
  return useMutation({ mutationFn: (reason: string) => apiRequest(`/api/v1/handoffs/${id}/revoke`, { method: 'POST', body: jsonBody({ reason_code: reason }), idempotent: true }), onSuccess: () => client.invalidateQueries({ queryKey: keys.handoffs }) });
}

export function useMandateToggle(id: UUID, enabled: boolean) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: () => apiRequest(`/api/v1/portfolio-mandates/${id}/${enabled ? 'disable' : 'enable'}`, { method: 'POST', body: jsonBody({}), idempotent: true }),
    onSuccess: async () => { await Promise.all([client.invalidateQueries({ queryKey: keys.mandates }), client.invalidateQueries({ queryKey: keys.portfolioPrograms }), client.invalidateQueries({ queryKey: keys.readiness })]); },
  });
}

export function useCreateDataSource() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (payload: Partial<DataSource> & Record<string, unknown>) => apiRequest<DataSource>('/api/v1/data-sources', { method: 'POST', body: jsonBody(payload), idempotent: true }),
    onSuccess: async () => { await Promise.all([client.invalidateQueries({ queryKey: keys.dataSources }), client.invalidateQueries({ queryKey: keys.readiness }), client.invalidateQueries({ queryKey: keys.health })]); },
  });
}

export function useCreateDownstream() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (payload: Partial<DownstreamSystem> & Record<string, unknown>) => apiRequest<DownstreamSystem>('/api/v1/downstream-systems', { method: 'POST', body: jsonBody(payload), idempotent: true }),
    onSuccess: async () => { await Promise.all([client.invalidateQueries({ queryKey: keys.downstreams }), client.invalidateQueries({ queryKey: keys.readiness })]); },
  });
}
