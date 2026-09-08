import { App, Alert, Button, Card, Drawer, Form, Input, Select, Space, Table, Tabs, Typography } from 'antd';
import { PlusOutlined, ReloadOutlined } from '@ant-design/icons';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useRef, useState } from 'react';
import { api, ApiFailure, dataOf, displayTime, Intent } from './api';
import type { Schema } from './api';
import { ErrorNotice, NoData, Pager, QueryPanel, ResourceFacts, StateTag, useGuard, useOnline } from './ui';
import { Briefs } from './briefs';
import { Runs } from './runs';

type Project = Schema['ProjectView'];
type Fields = Pick<Project, 'name' | 'description' | 'state'>;
export function Projects() {
  const [history, setHistory] = useState<(string | undefined)[]>([undefined]);
  const [selected, setSelected] = useState<Project>();
  const [editing, setEditing] = useState<Project | 'new'>();
  const online = useOnline();
  const cursor = history.at(-1);
  const query = useQuery({ queryKey: ['projects', cursor], queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/projects', {
    params: { query: { cursor, limit: 25 } }, signal,
  })) });
  if (selected) return <Space orientation="vertical" size="large" className="full-width">
    <Button onClick={() => setSelected(undefined)}>返回研究列表</Button>
    <ProjectDetail id={selected.id} />
  </Space>;
  return <Space orientation="vertical" size="large" className="full-width">
    <div className="page-heading"><div><Typography.Title level={1}>研究</Typography.Title><Typography.Paragraph type="secondary">从可检验的研究假设开始。保存草稿不会启动实验或消耗模型预算。</Typography.Paragraph></div>
      <Button icon={<PlusOutlined aria-hidden />} type="primary" disabled={!online} onClick={() => setEditing('new')}>新建研究</Button></div>
    <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
      <Card extra={<Button icon={<ReloadOutlined aria-hidden />} aria-label="刷新" aria-busy={query.isFetching} loading={query.isFetching} onClick={() => { void query.refetch(); }}>刷新</Button>}>
        <Table<Project> rowKey="id" dataSource={query.data?.items} pagination={false} scroll={{ x: 760 }}
          locale={{ emptyText: <NoData text="尚无研究项目。新建项目后填写 Brief，不能把空列表视为已完成研究。" /> }}
          columns={[
            { title: '研究项目', dataIndex: 'name', key: 'name', render: (_, project) => <Button type="link" className="table-title" onClick={() => setSelected(project)}>{project.name}</Button> },
            { title: '状态', key: 'state', render: (_, project) => <StateTag value={project.state} /> },
            { title: 'Brief', key: 'brief', render: (_, project) => project.current_brief_id ? <Typography.Text className="break-word">{project.current_brief_id}</Typography.Text> : '尚未选择' },
            { title: '更新于', key: 'updated', render: (_, project) => displayTime(project.updated_at) },
            { title: '操作', key: 'edit', render: (_, project) => <Button disabled={!online || query.isError} onClick={() => setEditing(project)}>编辑</Button> },
          ]} />
      </Card>
      <Pager history={history} next={query.data?.next_cursor} loading={query.isFetching} move={setHistory} />
    </QueryPanel>
    {editing && <ProjectEditor project={editing === 'new' ? undefined : editing} close={() => setEditing(undefined)} />}
  </Space>;
}
function ProjectDetail({ id }: { id: string }) {
  const query = useQuery({ queryKey: ['project', id], queryFn: async ({ signal }) => dataOf(await api.GET('/api/v2/projects/{id}', { params: { path: { id } }, signal })) });
  return <QueryPanel pending={query.isPending} error={query.error} stale={!!query.data} reload={() => { void query.refetch(); }}>
    {query.data && <><Typography.Title level={1}>{query.data.name}</Typography.Title>
      <Space wrap><StateTag value={query.data.state} /><Typography.Text type="secondary">{query.data.description || '尚无研究说明'}</Typography.Text></Space>
      <ResourceFacts id={id} revision={query.data.revision} updated={query.data.updated_at} />
      <Tabs destroyOnHidden items={[
        { key: 'briefs', label: '研究 Brief', children: <Briefs projectId={id} /> },
        { key: 'runs', label: '运行记录', children: <Runs projectId={id} /> },
      ]} />
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
      await client.invalidateQueries({ queryKey: ['projects'] });
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
      <Alert showIcon type="info" title="保存项目仅修改研究组织信息，不会启动 Cycle 或冻结 Brief。" />
      {project && <ResourceFacts id={project.id} revision={project.revision} updated={project.updated_at} />}
      <ErrorNotice error={mutation.error} />
      {conflict && <Button onClick={() => { void client.invalidateQueries({ queryKey: ['projects'] }); dismiss(); }}>关闭编辑并重新载入当前版本</Button>}
      <Form form={form} layout="vertical" initialValues={project ?? { name: '', description: '', state: 'DRAFT' }} onValuesChange={() => setDirty(true)}
        onFinish={value => { if (!mutation.isPending && online && !conflict) mutation.mutate(value); }} disabled={mutation.isPending || !online || conflict}>
        <Form.Item name="name" label="研究名称" rules={[{ required: true, whitespace: true, max: 120 }]}><Input maxLength={120} /></Form.Item>
        <Form.Item name="description" label="研究说明" rules={[{ max: 8000 }]}><Input.TextArea autoSize={{ minRows: 4, maxRows: 12 }} maxLength={8000} showCount /></Form.Item>
        {project && <Form.Item name="state" label="项目状态" rules={[{ required: true }]}><Select options={[
          { value: 'DRAFT', label: '草稿' }, { value: 'ACTIVE', label: '启用' }, { value: 'PAUSED', label: '暂停' }, { value: 'ARCHIVED', label: '归档' },
        ]} /></Form.Item>}
        <Space wrap><Button htmlType="submit" type="primary" aria-label="保存项目" aria-busy={mutation.isPending} loading={mutation.isPending} disabled={!online || mutation.isPending || conflict}>保存项目</Button><Button disabled={mutation.isPending} onClick={dismiss}>取消</Button></Space>
      </Form>
    </Space>
  </Drawer>;
}
