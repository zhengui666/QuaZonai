import { useEffect, useRef, useState } from 'react';
import { Button, Form, Input, Select } from 'antd';
import { ArrowLeftIcon, ArrowRightIcon, ChartLineUpIcon } from '@phosphor-icons/react';
import { useQueryClient } from '@tanstack/react-query';
import type { CapitalAssumptions, DatasetView, FactorAllocation, FactorView, PortfolioView } from '../../../packages/contracts/src/index.ts';
import { decimalUnits, draftIsPersisted, readDraft, removeDraft, saveDraft, useCommand } from './commands';
import { useResource } from './data-hooks';
import { currentRoute, href, navigate, type Route } from './router';
import { DraftNotice, ErrorNotice, Loading, PendingCommand } from './ui';
import { CapitalFields, capitalFields, capitalInput, type CapitalFormFields } from './capacity';

type Fields = { name: string; datasetId: string; factorIds: string[]; weights: Record<string, string>; capital: string; participation?: string; maximumAgeSeconds?: string; scenarios?: string };
export function AtomicPortfolioBuilder({ route }: { route: Route }) {
  const cache = useQueryClient(); const factors = useResource<FactorView[]>('/factors'); const datasets = useResource<DatasetView[]>('/datasets');
  const [form] = Form.useForm<Fields>(); const initialized = useRef(false);
  const [draft] = useState(() => readDraft<Fields>('atomic-portfolio')); const [saved, setSaved] = useState(() => draftIsPersisted('atomic-portfolio')); const [error, setError] = useState<string>();
  const command = useCommand<PortfolioView>('portfolios', value => { removeDraft('atomic-portfolio'); void cache.invalidateQueries({ queryKey: ['/portfolios'] }); if (currentRoute().page === 'portfolios' && currentRoute().creating) navigate(`/portfolios/${encodeURIComponent(value.id)}`); });
  const ids = Form.useWatch('factorIds', form) as string[] | undefined; const datasetId = Form.useWatch('datasetId', form) as string | undefined;
  const weights = Form.useWatch('weights', form) as Record<string, string> | undefined;
  useEffect(() => {
    if (initialized.current || !factors.data || !datasets.data) return;
    initialized.current = true;
    if (command.values) { const allocations = command.values.factors as FactorAllocation[] | undefined; const policy = command.values.capacityPolicy as CapitalAssumptions | undefined;
      form.setFieldsValue({ ...command.values, factorIds: allocations?.map(row => row.factorId) ?? [], weights: Object.fromEntries(allocations?.map(row => [row.factorId, row.weight]) ?? []), ...capitalFields(policy) } as Fields); return; }
    const factorId = route.params.get('factor'); const initial = factorId ? factors.data.find(row => row.id === factorId) : undefined;
    const selected = [...(draft?.factorIds ?? [])]; const allocations = { ...draft?.weights };
    if (factorId && !selected.includes(factorId)) { selected.push(factorId); allocations[factorId] = selected.length === 1 ? '1' : ''; }
    form.setFieldsValue({ ...(initial ? readDraft<CapitalFormFields>(`factor-capacity:${initial.id}`) ?? capitalFields(initial.capacityAssumptions) : {}), ...(initial?.datasetId ? { datasetId: initial.datasetId } : {}), ...draft, factorIds: selected, weights: allocations });
  }, [factors.data, datasets.data]);
  const amounts = (ids ?? []).map(id => decimalUnits(weights?.[id] ?? ''));
  const sum = amounts.length && amounts.every((value): value is bigint => value !== undefined && value > 0n) ? amounts.reduce((a, b) => a + b, 0n) : undefined;
  return <div className="authoring-page"><a className="text-link back-link" href={href('/portfolios')}><ArrowLeftIcon size={14} />组合回测</a><div className="authoring-heading"><div className="authoring-symbol"><ChartLineUpIcon size={24} /></div><h1>创建组合</h1></div><ErrorNotice error={factors.error ?? datasets.error} retry={() => { void factors.refetch(); void datasets.refetch(); }} />
    {factors.isPending || datasets.isPending ? <Loading /> : factors.data && datasets.data && <Form form={form} layout="vertical" disabled={!!command.pending || command.busy} scrollToFirstError={{ focus: true }} onValuesChange={(_changed, value) => { setError(undefined); if (!command.pending) setSaved(saveDraft('atomic-portfolio', value)); }} onFinish={value => {
      if (sum !== 1_000_000_000_000_000_000n) { setError('权重合计须为 1'); return; }
      try { const capacityPolicy = value.participation || value.maximumAgeSeconds || value.scenarios ? capitalInput(value, value.capital) : undefined;
        void command.submit({ name: value.name, datasetId: value.datasetId, capital: value.capital, factors: value.factorIds.map(factorId => ({ factorId, weight: value.weights[factorId] })), ...(capacityPolicy ? { capacityPolicy } : {}) });
      } catch { setError('请填写精确资本情景、成交量参与率与正整数数据时差'); }
    }}><PendingCommand command={command} />
      <Form.Item label="组合名称" name="name" rules={[{ required: true, whitespace: true, message: '请输入组合名称' }]}><Input autoFocus /></Form.Item>
      <Form.Item label="数据集" name="datasetId" rules={[{ required: true, message: '请选择数据集' }]}><Select options={datasets.data.map(row => ({ value: row.id, label: `${row.name} / ${row.currency}`, disabled: !row.ready }))} /></Form.Item>
      <Form.Item label="原子因子" name="factorIds" rules={[{ required: true, type: 'array', min: 1, message: '请选择有原单因子独立证据的因子' }]}><Select mode="multiple" showSearch optionFilterProp="label" options={factors.data.map(row => ({ value: row.id, label: `${row.id} / ${row.name}${row.combinable ? '' : ` / ${row.reason}`}`, disabled: !row.combinable || !!datasetId && row.datasetId !== null && row.datasetId !== datasetId }))} onChange={(selected: string[]) => { if (selected.length === 1) { if (!form.getFieldValue(['weights', selected[0]!])) form.setFieldValue(['weights', selected[0]!], '1'); if (!form.getFieldValue('participation')) form.setFieldsValue(readDraft<CapitalFormFields>(`factor-capacity:${selected[0]}`) ?? capitalFields(factors.data!.find(row => row.id === selected[0])?.capacityAssumptions)); } }} /></Form.Item>
      <p className="quiet">每项是原始研究 / 试验 / 单模型。组合组件证据不冒充独立单因子表现；一次原生模拟共用总本金。</p>
      {(ids ?? []).map(id => { const factor = factors.data!.find(row => row.id === id); return <div className="member-row" key={id}><div><h3><a href={href(`/factors/${encodeURIComponent(id)}`)}>{factor?.name ?? '来源名称暂不可用'}</a></h3><p className="quiet">{factor?.hypothesis}</p>{factor && <a className="text-link" href={href(`/research/${factor.researchId}?tab=trials&trial=${factor.trialId}`)}>原研究 #{factor.researchId} / 试验 #{factor.trialId}</a>}<code>{id}</code></div><Form.Item label={`因子 ${id} 权重`} name={['weights', id]} rules={[{ required: true, message: '请输入权重' }, { validator: (_rule, value: string) => (decimalUnits(value ?? '') ?? 0n) > 0n ? Promise.resolve() : Promise.reject(new Error('请输入精确正小数')) }]}><Input inputMode="decimal" /></Form.Item></div>; })}
      <p className="allocation-total" role={error ? 'alert' : 'status'}>{sum === 1_000_000_000_000_000_000n ? '权重合计 1' : '正权重合计须为 1'}{error && <span className="error-text">{error}</span>}</p>
      <Form.Item label="本金" name="capital" rules={[{ required: true, message: '请输入本金' }, { validator: (_rule, value: string) => (decimalUnits(value ?? '') ?? 0n) > 0n ? Promise.resolve() : Promise.reject(new Error('请输入精确正金额')) }]}><Input inputMode="decimal" suffix={datasets.data.find(row => row.id === datasetId)?.currency} /></Form.Item>
      <details className="provenance-disclosure"><summary>条件容量假设（可选）</summary><CapitalFields /><p className="quiet">本次明确假设独立于成员研究，适用于组合净需求；均留空时仅继承一致的原研究假设。只使用决策前可得的最后一根 BAR，费用与执行映射沿用原始政策，不推算未来盘口。</p></details>
      <ErrorNotice error={command.error} /><DraftNotice saved={saved || !!command.pending} /><div className="form-actions"><Button disabled={false} onClick={() => navigate('/portfolios')}>取消</Button>{command.pending ? <Button type="primary" loading={command.busy} onClick={() => { void command.submit(); }}>核对提交</Button> : <Button type="primary" htmlType="submit">运行回测 <ArrowRightIcon size={15} /></Button>}</div>
    </Form>}</div>;
}
