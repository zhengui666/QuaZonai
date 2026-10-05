import { App, Button, Grid, Input, Select, Space, Tabs, Typography } from 'antd';
import { ArrowRightOutlined, ArrowLeftOutlined, ExperimentOutlined, FileTextOutlined, ReloadOutlined, SearchOutlined } from '@ant-design/icons';
import { useQuery } from '@tanstack/react-query';
import { useContext, useState } from 'react';
import { api, dataOf, displayTime } from './api';
import type { Schema } from './api';
import { GuardContext, Pager, QueryPanel, ResourceFacts, StateTag } from './ui';
import { Briefs } from './briefs';
import { DataInputs } from './data-inputs';
import { ResearchOverview } from './research-overview';
import { Runs } from './runs';
import { Cycles } from './cycles';
import { AgentEvaluations } from './agent-evaluations';

type Project = Schema['ProjectView'];
export function Projects({ onNavigate }: { onNavigate?: (section: string) => void }) {
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]);
  const [selected, setSelected] = useState<Project>();
  const [search, setSearch] = useState('');
  const { blocked } = useContext(GuardContext); const { message } = App.useApp();
  function returnToProjects() {
    if (blocked) { void message.info('请先完成、保存或取消当前操作'); return; }
    setSelected(undefined);
  }
  const cursor = history.at(-1);
  const query = useQuery({ queryKey: ['projects', cursor], queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/projects', {
    params: { query: { cursor, limit: 25 } }, signal,
  })) });
  const projects = query.data?.items ?? [];
  const visible = projects.filter(project => `${project.name} ${project.description ?? ''}`.toLocaleLowerCase().includes(search.trim().toLocaleLowerCase()));
  if (selected) return <Space orientation="vertical" size="large" className="full-width">
    <Button type="text" icon={<ArrowLeftOutlined aria-hidden />} onClick={returnToProjects}>返回研究列表</Button>
    <ProjectDetail id={selected.id} />
  </Space>;
  return <div className="research-workspace">
    <header className="research-heading">
      <div><div className="eyebrow">RESEARCH WORKSPACE</div><Typography.Title level={1}>研究</Typography.Title>
        <p>只读查看研究与证据；创建和修改由外部 Agent 通过 CLI/Skill 执行。</p></div>
    </header>
    <section className="research-intro" aria-label="研究工作流">
      <div className="intro-copy"><span className="eyebrow">从想法到证据</span><h2>定义 · 实验<br />验证 · 交付</h2><p>定义目标与预算，冻结研究输入，再通过独立评估检验假设。</p></div>
      <ol className="research-pipeline">
        <li><span className="step-number">01</span><div><h3>定义问题</h3><p>建立项目，明确研究 Brief</p></div><FileTextOutlined aria-hidden /></li>
        <li><span className="step-number">02</span><div><h3>开展实验</h3><p>冻结输入，推进研究周期</p></div><ExperimentOutlined aria-hidden /></li>
        <li><span className="step-number">03</span><div><h3>验证与交付</h3><p>审查证据，形成目标组合</p></div><ArrowRightOutlined aria-hidden /></li>
      </ol>
    </section>
    <div className="workspace-columns">
      <section className="project-collection" aria-labelledby="project-collection-title">
        <div className="collection-heading"><div><h2 id="project-collection-title">研究项目</h2><p>继续已有研究，或探索新的假设</p></div>
          <Button type="text" icon={<ReloadOutlined aria-hidden />} aria-label="刷新" aria-busy={query.isFetching} loading={query.isFetching} onClick={() => { void query.refetch(); }}>刷新</Button></div>
        <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
          {projects.length > 0 && <div className="project-tools"><Input prefix={<SearchOutlined aria-hidden />} aria-label="搜索本页研究项目" placeholder="搜索本页项目" value={search} allowClear onChange={event => setSearch(event.target.value)} /><span>{projects.length} 个项目 · 本页</span></div>}
          {projects.length === 0 ? <div className="research-empty"><span className="empty-symbol"><ExperimentOutlined aria-hidden /></span><h3>尚无研究项目</h3><p>由外部 Agent 通过 CLI/Skill 登记研究后，这里展示原始目标、过程与评估证据。</p></div> : visible.length === 0 ? <div className="research-empty"><h3>本页没有匹配的项目</h3><Button onClick={() => setSearch('')}>清除搜索</Button></div> : <div className="project-grid">{visible.map(project => <article className="research-project" key={project.id}>
            <div className="project-card-top"><span className="project-symbol"><ExperimentOutlined aria-hidden /></span><StateTag value={project.state} /></div>
            <h3><button className="project-title" onClick={() => setSelected(project)}>{project.name}</button></h3>
            <p className="project-description">{project.description || '尚未填写研究说明。打开项目，定义研究目标与约束。'}</p>
            <div className="project-brief" title={project.current_brief_id ?? undefined}><span className={project.current_brief_id ? 'brief-dot ready' : 'brief-dot'} />{project.current_brief_id ? '已关联研究 Brief' : '下一步：建立研究 Brief'}</div>
            <div className="project-card-bottom"><time dateTime={project.updated_at}>更新于 {displayTime(project.updated_at)}</time></div>
            <Button className="project-open" onClick={() => setSelected(project)}>进入研究 <ArrowRightOutlined aria-hidden /></Button>
          </article>)}</div>}
          {(history.length > 1 || query.data?.next_cursor) && <Pager history={history} next={query.data?.next_cursor} loading={query.isFetching} move={next => { setHistory(next); setSearch(''); }} />}
        </QueryPanel>
      </section>
      <aside className="research-side" aria-label="研究资源">

        {onNavigate && <nav className="workspace-shortcuts" aria-label="研究快捷入口"><h3>工作台入口</h3>{[
          ['alpha', 'Alpha 证据', '查看研究成果与评估'], ['portfolio', '组合记录', '查看候选策略与目标组合'], ['runs', '运行记录', '追踪执行、状态与回执'],
        ].map(([key, title, description]) => <button key={key} onClick={() => onNavigate(key!)}><span><strong>{title}</strong><small>{description}</small></span><ArrowRightOutlined aria-hidden /></button>)}</nav>}
      </aside>
    </div>
  </div>;
}
export function ProjectDetail({ id }: { id: string }) {
  const [tab, setTab] = useState('overview');
  const screens = Grid.useBreakpoint();
  const { blocked } = useContext(GuardContext); const { message } = App.useApp();
  function changeTab(next: string) {
    if (next === tab) return;
    if (blocked) { void message.info('请先完成、保存或取消当前操作'); return; }
    setTab(next);
  }
  const query = useQuery({ queryKey: ['project', id], queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/projects/{id}', { params: { path: { id } }, signal })) });
  const projectState = query.isError || query.isFetching ? undefined : query.data?.state;
  const sections = query.data ? [
        { key: 'overview', label: '工作概览', children: <ResearchOverview project={query.data} current={projectState !== undefined} navigate={changeTab} /> },
        { key: 'briefs', label: '研究 Brief', children: <Briefs projectId={id} projectState={projectState} currentBriefId={query.data.current_brief_id} /> },
        { key: 'inputs', label: '冻结输入', children: <DataInputs projectId={id} /> },
        { key: 'cycles', label: '研究周期', children: <Cycles projectId={id} /> },
        { key: 'runs', label: '运行记录', children: <Runs projectId={id} /> },
        { key: 'agent-evaluations', label: 'Agent 评估', children: <AgentEvaluations projectId={id} /> },
      ] : [];
  return <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
    {query.data && <div className="project-workspace">
      <header className="project-detail-heading"><div><span className="eyebrow">RESEARCH PROJECT</span><Typography.Title level={1}>{query.data.name}</Typography.Title>
        <div className="project-detail-status"><StateTag value={query.data.state} /><span>更新于 {displayTime(query.data.updated_at)}</span></div>
        {query.data.description && <p className="project-detail-description">{query.data.description}</p>}</div>
      </header>
      <details className="record-details"><summary>项目记录与修订</summary><ResourceFacts id={id} revision={query.data.revision} updated={query.data.updated_at} /></details>
      {!screens.md && <Select className="full-width research-section-select" aria-label="研究项目章节" virtual={false} value={tab} onChange={changeTab} options={sections.map(({ key, label }) => ({ value: key, label }))} />}
      <Tabs key="project-sections" className="research-tabs" activeKey={tab} onChange={changeTab} destroyOnHidden items={sections} />
    </div>}
  </QueryPanel>;
}
