import { Button } from 'antd';
import { ArrowRightOutlined } from '@ant-design/icons';
import { useQuery } from '@tanstack/react-query';
import { api, ApiFailure, dataOf } from './api';
import type { Schema } from './api';
import { QueryPanel, StateTag } from './ui';

export function ResearchOverview({ project, current, navigate }: {
  project: Schema['ProjectView']; current: boolean; navigate: (section: string) => void;
}) {
  const briefId = project.current_brief_id;
  const brief = useQuery({ queryKey: ['briefs', project.id, 'current', briefId], enabled: !!briefId,
    queryFn: async ({ signal }) => {
      if (!briefId) throw new ApiFailure('BRIEF_REQUIRED', '尚未选择当前 Brief');
      const result = dataOf(await api.GET('/api/v2/briefs/{id}', { params: { path: { id: briefId } }, signal }));
      if (result.project_id !== project.id || result.id !== briefId) throw new ApiFailure('HTTP_CONTRACT_ERROR', 'Brief 不属于当前研究项目');
      return result;
    } });
  const cycles = useQuery({ queryKey: ['cycles', project.id, undefined], refetchInterval: 10_000,
    queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/projects/{id}/cycles', {
      params: { path: { id: project.id }, query: { limit: 25 } }, signal,
    })) });
  return <div className="research-overview">
    <section className="current-hypothesis" aria-labelledby="current-hypothesis-heading">
      <div className="section-toolbar"><div><span className="eyebrow">研究问题</span><h2 id="current-hypothesis-heading">当前 Brief</h2></div><Button onClick={() => navigate('briefs')}>管理 Brief <ArrowRightOutlined aria-hidden /></Button></div>
      {!briefId ? <div className="overview-empty"><h3>尚未选择当前 Brief</h3><p>查看已有版本或建立新草稿，定义可检验的假设。这里不会将列表中的其他版本视为当前版本。</p></div> : <QueryPanel pending={brief.isPending} error={brief.error} stale={!!brief.data} reload={() => { void brief.refetch(); }}>
        {brief.data && <><div className="record-label"><StateTag value={brief.data.state} /><span>版本 {brief.data.version}</span>{!current && <span>项目状态正在核对</span>}</div><h3 className="hypothesis-text">{brief.data.content.hypothesis}</h3><p className="brief-rationale">{brief.data.content.economic_rationale}</p><div className="record-meta"><span>基础币种：{brief.data.content.base_currency}</span><span>数据绑定：{brief.data.bindings.length}</span><span>修订：{brief.data.revision}</span></div></>}
      </QueryPanel>}
    </section>
    <nav className="research-stages" aria-label="项目工作流">
      {([
        ['inputs', '01', '准备冻结输入', '查看项目输入与数据质量验证，保留用途与时间边界'],
        ['briefs', '02', '确定执行上下文', '编辑草稿或查看冻结版本，再明确选择 Runtime 和输入'],
        ['cycles', '03', '推进研究周期', '核对周期实际结果、下一步与冻结试验选择'],
      ] as const).map(([key, number, title, description]) => <button key={key} onClick={() => navigate(key)}><span className="step-number">{number}</span><strong>{title}</strong><span>{description}</span><ArrowRightOutlined aria-hidden /></button>)}
    </nav>
    <section className="overview-cycles" aria-labelledby="overview-cycles-heading"><div className="section-toolbar"><div><h2 id="overview-cycles-heading">研究周期与下一步</h2><p>呈现服务端记录，不把执行成功等同于证据通过。</p></div><Button onClick={() => navigate('cycles')}>查看研究周期</Button></div>
      <QueryPanel pending={cycles.isPending} error={cycles.error} stale={!!cycles.data} reload={() => { void cycles.refetch(); }}>
        {cycles.data?.items.length === 0 ? <div className="overview-empty"><h3>尚无研究周期</h3><p>在 Brief 中确认冻结上下文。启用项目后，选择研究者与独立 Reviewer，明确确认启动。</p><Button onClick={() => navigate('briefs')}>查看 Brief 与启动条件</Button></div> : <>
          <div className="overview-cycle-list">{cycles.data?.items.slice(0, 3).map(cycle => <article key={cycle.id}><div><StateTag value={cycle.state} /><span className="break-word">{cycle.id}</span></div><p>{cycle.outcome ?? '尚无周期结论'}</p><strong>{cycle.next_action ?? '尚无下一步记录'}</strong></article>)}</div>
          {cycles.data && (cycles.data.items.length > 3 || cycles.data.next_cursor) && <p className="section-caption">仅展示当前页前 3 条；全部记录与分页见研究周期。</p>}
        </>}
      </QueryPanel>
    </section>
  </div>;
}
