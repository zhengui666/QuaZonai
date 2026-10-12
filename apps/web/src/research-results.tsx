import { Alert, Descriptions, Space, Tag, Typography } from 'antd';
import { hypothesisOf, nativeId, nativeText, object } from './research-native';
import type { NativeObject } from './research-native';

export function NativeJson({ label, value }: { label: string; value: unknown }) {
  return <details className="native-json"><summary>{label}</summary>
    <pre tabIndex={0} aria-label={label}>{JSON.stringify(value, null, 2) ?? '未报告'}</pre>
  </details>;
}
export function NativeStatus({ value }: { value: unknown }) {
  const status = nativeText(value);
  // COMPUTED is a computation fact, never a scientific pass/qualification.
  return <Tag color={status === 'FAILED' || status === 'REJECTED' ? 'error' : status === 'STARTED' ? 'processing' : undefined}>{status}</Tag>;
}
function Metrics({ value }: { value: unknown }) {
  const metric = object(value); const ic = object(metric?.pearson_ic); const counts = object(metric?.counts);
  return <Descriptions size="small" column={1} items={[
    { key: 'ic-status', label: 'pearson_ic.status', children: nativeText(ic?.status) },
    { key: 'ic-value', label: 'pearson_ic.value', children: <strong>{nativeText(ic?.value)}</strong> },
    ...(counts ? Object.entries(counts).map(([key, count]) => ({ key: `count-${key}`, label: `counts.${key}`, children: nativeText(count) }))
      : [{ key: 'counts', label: 'counts', children: nativeText(metric?.counts) }]),
  ]} />;
}
export function NativeResult({ value, label }: { value: unknown; label: string }) {
  const result = object(value);
  if (!result) return <Typography.Paragraph type="secondary">{value === null ? '尚无计算摘要（null），不补零。' : '尚无计算摘要。'}</Typography.Paragraph>;
  const partitions = Array.isArray(result.partitions) ? result.partitions : undefined;
  return <Space orientation="vertical" className="full-width" size="middle">
    {result.consumed_fuel !== undefined && <Typography.Text>consumed_fuel：{nativeText(result.consumed_fuel)}</Typography.Text>}
    {partitions && <div className="native-partitions">{partitions.map((item, index) => {
      const partition = object(item);
      if (!partition) return <NativeJson key={index} label={`原生 partition ${index + 1}`} value={item} />;
      const assets = Array.isArray(partition.assets) ? partition.assets : undefined;
      return <section key={`${nativeText(partition.partition)}-${index}`} className="native-partition" aria-label={`${nativeText(partition.partition)} 指标`}>
        <h4>{nativeText(partition.partition)}</h4>
        <p className="section-caption break-word">区间（原生 ns）：{nativeText(partition.start_ns)} — {nativeText(partition.end_ns)}</p>
        <Metrics value={partition.metric} />
        {assets && <details className="native-json"><summary>逐资产指标 · {assets.length} 项</summary>
          {assets.length === 0 ? <p className="section-caption">原生 assets 为空，没有资产指标。</p> : assets.map((asset, assetIndex) => {
            const row = object(asset);
            return <section className="native-asset" key={assetIndex}><h5>{nativeText(row?.instrument_id)}</h5><Metrics value={row?.metric} /></section>;
          })}
        </details>}
      </section>;
    })}</div>}
    <NativeJson label={label} value={value} />
  </Space>;
}
export function ResearchTrials({ trials }: { trials: NativeObject[] }) {
  return <section aria-labelledby="research-trials-title">
    <div className="section-toolbar"><div><h2 id="research-trials-title">试验与假设</h2><p>原生计算状态与指标摘要；COMPUTED 不代表证据通过。</p></div><Tag>{trials.length} 条记录</Tag></div>
    {trials.length === 0 ? <div className="durable-empty"><h3>尚无试验记录</h3><p>任务执行与插件记录会在此显示。提交被接纳不代表已开展计算。</p></div>
      : <div className="native-trials">{trials.map((trial, index) => <article className="native-trial" key={nativeId(trial.taskId) ? trial.taskId : index}
        id={nativeId(trial.taskId) ? `trial-${trial.taskId}` : undefined}>
        <div className="record-label"><span>Task #{nativeText(trial.taskId)}</span><NativeStatus value={trial.status} /></div>
        <h3>{hypothesisOf(trial.proposal) ?? '插件未提供 hypothesis 字段'}</h3>
        {trial.error !== undefined && trial.error !== null && <Alert type="error" showIcon title="原生失败 / 拒绝原因" description={<span className="break-word">{nativeText(trial.error)}</span>} />}
        <NativeJson label={`试验 #${nativeText(trial.taskId)} · 完整提案`} value={trial.proposal} />
        <NativeResult value={trial.result} label={`试验 #${nativeText(trial.taskId)} · 原生结果 JSON`} />
        <NativeJson label={`试验 #${nativeText(trial.taskId)} · 原生记录 JSON`} value={trial} />
      </article>)}</div>}
  </section>;
}
export function ResearchFinal({ value }: { value: unknown }) {
  const final = object(value);
  return <section className="native-final" aria-labelledby="research-final-title">
    <div className="section-toolbar"><div><h2 id="research-final-title">选择与最终评估</h2><p>仅所有者可见的最终摘要；不作为模型调优反馈。</p></div>{final && <NativeStatus value={final.status} />}</div>
    {!final ? <p className="section-caption">尚未记录最终选择或结果。没有选择不等于失败，也不推定最优试验。</p> : <Space orientation="vertical" className="full-width" size="middle">
      <Descriptions size="small" column={1} items={[
        { key: 'task', label: '最终 Task', children: nativeText(final.taskId) },
        { key: 'selected', label: '选择的试验', children: nativeId(final.trialId) ? <a href={`#trial-${final.trialId}`}>Task #{final.trialId}</a> : nativeText(final.trialId) },
        { key: 'reason', label: '选择 / 停止理由', children: nativeText(final.reason) },
        { key: 'compared', label: 'comparedTrials（原生）', children: nativeText(final.comparedTrials) },
      ]} />
      {final.error !== undefined && final.error !== null && <Alert type="error" showIcon title="原生最终评估失败原因" description={nativeText(final.error)} />}
      <NativeResult value={final.result} label="所有者最终结果 · 原生 JSON" />
      <NativeJson label="所有者最终记录 · 原生 JSON" value={final} />
    </Space>}
  </section>;
}
export function NativeUsage({ value, label }: { value: unknown; label: string }) {
  const usage = object(value); const models = object(usage?.models); const tools = object(usage?.tools);
  const rows = [...Object.entries(models ?? {}).map(([key, item]) => ({ key, kind: 'model', item })),
    ...Object.entries(tools ?? {}).map(([key, item]) => ({ key, kind: 'tool', item }))];
  return <section aria-label={label}>
    <p className="section-caption">原生累计测量；未报告或 null 不等于 0。reasoning 是 output 的子集，不重复相加。cost 保留 Host 原生单位。</p>
    {rows.length === 0 ? <p className="section-caption">尚无原生用量记录，不推定零用量。</p> : <div className="native-usage">{rows.map(({ key, kind, item }) => {
      const row = object(item); const cost = object(row?.cost);
      return <article key={`${kind}-${key}`}><h4>{kind} · {key}</h4><Descriptions size="small" column={{ xs: 1, sm: 2 }} items={[
        ...['input', 'output', 'cacheRead', 'cacheWrite', 'reasoning', 'totalTokens'].map(field => ({ key: field, label: field, children: nativeText(row?.[field]) })),
        { key: 'cost', label: 'cost.total（原生）', children: nativeText(cost?.total) },
      ]} /><NativeJson label={`${kind} · ${key} · 完整用量`} value={item} /></article>;
    })}</div>}
    <NativeJson label={`${label} · 原生 JSON`} value={value} />
  </section>;
}
