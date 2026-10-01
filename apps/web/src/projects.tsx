import { App, Button, Drawer, Form, Input, Select, Space, Tabs, Typography } from 'antd';
import { ArrowRightOutlined, ArrowLeftOutlined, ExperimentOutlined, FileTextOutlined, PlusOutlined, ReloadOutlined, SearchOutlined } from '@ant-design/icons';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useRef, useState } from 'react';
import { api, ApiFailure, dataOf, displayTime, Intent } from './api';
import type { Schema } from './api';
import { ErrorNotice, Pager, QueryPanel, ResourceFacts, StateTag, useGuard, useOnline } from './ui';
import { Briefs } from './briefs';
import { Runs } from './runs';
import { Cycles } from './cycles';
import { AgentEvaluations } from './agent-evaluations';
import { projectStateOptions } from './authoring-options';

type Project = Schema['ProjectView'];
type Fields = Pick<Project, 'name' | 'description' | 'state'>;
export function Projects({ onNavigate }: { onNavigate?: (section: string) => void }) {
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]);
  const [selected, setSelected] = useState<Project>();
  const [editing, setEditing] = useState<Project | 'new'>();
  const [search, setSearch] = useState('');
  const online = useOnline();
  const cursor = history.at(-1);
  const query = useQuery({ queryKey: ['projects', cursor], queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/projects', {
    params: { query: { cursor, limit: 25 } }, signal,
  })) });
  const projects = query.data?.items ?? [];
  const visible = projects.filter(project => `${project.name} ${project.description ?? ''}`.toLocaleLowerCase().includes(search.trim().toLocaleLowerCase()));
  if (selected) return <Space orientation="vertical" size="large" className="full-width">
    <Button type="text" icon={<ArrowLeftOutlined aria-hidden />} onClick={() => setSelected(undefined)}>返回研究列表</Button>
    <ProjectDetail id={selected.id} />
  </Space>;
  return <div className="research-workspace">
    <header className="research-heading">
      <div><div className="eyebrow">RESEARCH WORKSPACE</div><Typography.Title level={1}>研究</Typography.Title>
        <p>从一个好问题开始，让每一步都有证据。</p></div>
      <Button icon={<PlusOutlined aria-hidden />} type="primary" disabled={!online} onClick={() => setEditing('new')}>新建研究</Button>
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
          {projects.length === 0 ? <div className="research-empty"><span className="empty-symbol"><ExperimentOutlined aria-hidden /></span><h3>你的下一个研究，从这里开始</h3><p>为一个假设建立项目。研究目标、实验过程与评估证据，都将在这里归档。</p><Button disabled={!online} onClick={() => setEditing('new')}>创建第一个研究 <ArrowRightOutlined aria-hidden /></Button></div> : visible.length === 0 ? <div className="research-empty"><h3>本页没有匹配的项目</h3><Button onClick={() => setSearch('')}>清除搜索</Button></div> : <div className="project-grid">{visible.map(project => <article className="research-project" key={project.id}>
            <div className="project-card-top"><span className="project-symbol"><ExperimentOutlined aria-hidden /></span><StateTag value={project.state} /></div>
            <h3><button className="project-title" onClick={() => setSelected(project)}>{project.name}</button></h3>
            <p className="project-description">{project.description || '尚未填写研究说明。打开项目，定义研究目标与约束。'}</p>
            <div className="project-brief" title={project.current_brief_id ?? undefined}><span className={project.current_brief_id ? 'brief-dot ready' : 'brief-dot'} />{project.current_brief_id ? '已关联研究 Brief' : '下一步：建立研究 Brief'}</div>
            <div className="project-card-bottom"><time dateTime={project.updated_at}>更新于 {displayTime(project.updated_at)}</time><Button type="text" disabled={!online || query.isError} onClick={() => setEditing(project)}>编辑</Button></div>
            <Button className="project-open" onClick={() => setSelected(project)}>进入研究 <ArrowRightOutlined aria-hidden /></Button>
          </article>)}</div>}
          {(history.length > 1 || query.data?.next_cursor) && <Pager history={history} next={query.data?.next_cursor} loading={query.isFetching} move={next => { setHistory(next); setSearch(''); }} />}
        </QueryPanel>
      </section>
      <aside className="research-side" aria-label="研究资源">
        <div className="workspace-note"><span className="eyebrow">研究原则</span><h2>证据先行。<br />边界清晰。</h2><p>计算成功不等于证据通过。保留独立评估、试验账本与预算约束，让结论可追溯。</p><div className="note-footer"><span className="brief-dot ready" />仅交付目标组合，不发送券商订单</div></div>
        {onNavigate && <nav className="workspace-shortcuts" aria-label="研究快捷入口"><h3>工作台入口</h3>{[
          ['alpha', 'Alpha 证据', '查看研究成果与评估'], ['portfolio', '组合构建', '从候选策略到目标组合'], ['runs', '运行记录', '追踪执行、状态与回执'],
        ].map(([key, title, description]) => <button key={key} onClick={() => onNavigate(key!)}><span><strong>{title}</strong><small>{description}</small></span><ArrowRightOutlined aria-hidden /></button>)}</nav>}
      </aside>
    </div>
    {editing && <ProjectEditor project={editing === 'new' ? undefined : editing} close={() => setEditing(undefined)} />}
  </div>;
}
function ProjectDetail({ id }: { id: string }) {
  const [editing, setEditing] = useState<Project>(); const online = useOnline();
  const query = useQuery({ queryKey: ['project', id], queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/projects/{id}', { params: { path: { id } }, signal })) });
  return <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
    {query.data && <><Typography.Title level={1}>{query.data.name}</Typography.Title>
      <Space wrap><StateTag value={query.data.state} /><Typography.Text type="secondary">{query.data.description || ''}</Typography.Text></Space>
      <ResourceFacts id={id} revision={query.data.revision} updated={query.data.updated_at} />
      <Button disabled={!online || query.isError || query.isFetching} onClick={() => setEditing(query.data)}>修改项目状态</Button>
      <Tabs destroyOnHidden items={[
        { key: 'briefs', label: '研究 Brief', children: <Briefs projectId={id} projectState={query.isError || query.isFetching ? undefined : query.data.state} /> },
        { key: 'cycles', label: '研究周期', children: <Cycles projectId={id} /> },
        { key: 'runs', label: '运行记录', children: <Runs projectId={id} /> },
        { key: 'agent-evaluations', label: 'Agent 评估', children: <AgentEvaluations projectId={id} /> },
      ]} />
      {editing && <ProjectEditor project={editing} close={() => setEditing(undefined)} />}
    </>}
  </QueryPanel>;
}
function ProjectEditor({ project, close }: { project?: Project; close: () => void }) {
  const [form] = Form.useForm<Fields>();
  const [dirty, setDirty] = useState(false);
  const intent = useRef(new Intent());
  const client = useQueryClient();
  const online = useOnline();
  const { message, modal } = App.useApp();
  const currentBriefId = project?.current_brief_id;
  const currentBrief = useQuery({
    queryKey: ['project-state-brief', project?.id, currentBriefId],
    enabled: !!currentBriefId && project?.state !== 'ARCHIVED', retry: false,
    queryFn: async ({ signal }) => {
      if (!currentBriefId) throw new ApiFailure('BRIEF_REQUIRED', '项目尚无当前 Brief。');
      return dataOf(await api.GET('/api/v2/briefs/{id}', { params: { path: { id: currentBriefId } }, signal }));
    },
  });
  const states = project ? projectStateOptions(project, currentBrief.isError ? undefined : currentBrief.data) : [];
  const mutation = useMutation({
    mutationFn: async (value: Fields) => {
      if (project) {
        const body: Schema['ProjectUpdate'] = { schema_version: 1, expected_revision: project.revision, name: value.name, description: value.description, state: value.state };
        return dataOf(await api.PATCH('/api/v2/projects/{id}', { params: { path: { id: project.id }, header: intent.current.headers('PATCH', `/api/v2/projects/${project.id}`, body) }, body }));
      }
      const body: Schema['ProjectCreate'] = { schema_version: 1, name: value.name, description: value.description, fork_from_project_id: null };
      return dataOf(await api.POST('/api/v2/projects', { params: { header: intent.current.headers('POST', '/api/v2/projects', body) }, body }));
    },
    onSuccess: async result => {
      intent.current.clear(); setDirty(false);
      await Promise.all([client.invalidateQueries({ queryKey: ['projects'] }),
        client.invalidateQueries({ queryKey: ['project', project?.id] })]);
      await message.success(result.replayed ? '已读取上次操作的结果，没有重复创建。' : '研究项目已保存。');
      close();
    },
  });
  useGuard(dirty || mutation.isPending);
  const conflict = mutation.error instanceof ApiFailure && mutation.error.code === 'REVISION_CONFLICT';
  function dismiss() {
    if (mutation.isPending) return;
    if (!dirty) { close(); return; }
    modal.confirm({ title: '放弃尚未保存的修改？', content: '这不会撤销已经到达服务器的请求。', okText: '放弃修改', cancelText: '继续编辑', onOk: close });
  }
  return <Drawer title={project ? '编辑研究项目' : '新建研究项目'} open onClose={dismiss} maskClosable={!mutation.isPending} closable={!mutation.isPending} width={600}>
    <Space orientation="vertical" className="full-width" size="middle">
      
      {project && <ResourceFacts id={project.id} revision={project.revision} updated={project.updated_at} />}
      <ErrorNotice error={mutation.error} />
      {conflict && <Button onClick={() => { void Promise.all([client.invalidateQueries({ queryKey: ['projects'] }), client.invalidateQueries({ queryKey: ['project', project?.id] })]); dismiss(); }}>关闭编辑并重新载入当前版本</Button>}
      <Form form={form} layout="vertical" initialValues={project ?? { name: '', description: '', state: 'DRAFT' }} onValuesChange={() => setDirty(true)}
        onFinish={value => { if (!mutation.isPending && online && !conflict) mutation.mutate(value); }} disabled={mutation.isPending || !online || conflict}>
        <Form.Item name="name" label="研究名称" rules={[{ required: true, whitespace: true, max: 120 }]}><Input maxLength={120} /></Form.Item>
        <Form.Item name="description" label="研究说明" rules={[{ max: 8000 }]}><Input.TextArea autoSize={{ minRows: 4, maxRows: 12 }} maxLength={8000} showCount /></Form.Item>
        {project && <Form.Item name="state" label="项目状态"
          rules={[{ required: true }, { validator: (_, value: unknown) => states.some(option => option.value === value)
            ? Promise.resolve() : Promise.reject(new Error('当前项目或 Brief 状态不允许此选择，请重新选择。')) }]}>
          <Select options={states} loading={!!currentBriefId && currentBrief.isFetching} />
        </Form.Item>}
        {currentBriefId && project?.state !== 'ARCHIVED' && <ErrorNotice error={currentBrief.error} retry={() => { void currentBrief.refetch(); }} />}
        <Space wrap><Button htmlType="submit" type="primary" aria-label="保存项目" aria-busy={mutation.isPending} loading={mutation.isPending} disabled={!online || mutation.isPending || conflict}>保存项目</Button><Button disabled={mutation.isPending} onClick={dismiss}>取消</Button></Space>
      </Form>
    </Space>
  </Drawer>;
}
