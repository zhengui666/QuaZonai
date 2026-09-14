import { Alert, App, Button, Descriptions, Modal, Space, Typography } from 'antd';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useRef, useState } from 'react';
import { api, ApiFailure, dataOf, displayTime, Intent } from './api';
import type { Schema } from './api';
import { ErrorNotice, useGuard, useOnline } from './ui';
import { ReleaseDetail } from './delivery';

export function ReleaseCreate({ project, candidate, evaluation, close }: {
  project: string; candidate: string; evaluation: Schema['EvaluationView']; close: () => void;
}) {
  const intent = useRef(new Intent()); const hadUnknown = useRef(false);
  const [submitted, setSubmitted] = useState<Schema['ReleaseCreateV1']>();
  const [receipt, setReceipt] = useState<Schema['ReleaseViewV1']>(); const [detail, setDetail] = useState(false);
  const online = useOnline(); const client = useQueryClient(); const { modal } = App.useApp();
  const eligible = evaluation.project_id === project && evaluation.subject_candidate_id === candidate && evaluation.subject_alpha_version_id === null
    && evaluation.evaluation_kind === 'PORTFOLIO' && evaluation.execution_status === 'SUCCEEDED'
    && evaluation.evidence_status === 'VALID' && evaluation.decision === 'PASS' && evaluation.origin === 'REAL' && evaluation.unexpired_at_read;
  const mutation = useMutation({ mutationFn: async (body: Schema['ReleaseCreateV1']) => {
    const result = dataOf(await api.POST('/api/v2/releases', { body, params: { header: intent.current.headers('POST', '/api/v2/releases', body) } }));
    if (result.resource.project_id !== project || result.resource.candidate_id !== body.candidate_id || result.resource.evaluation_id !== body.evaluation_id) throw new Error('返回的目标包不属于原冻结请求。');
    return result;
  }, onSuccess: async result => {
    setReceipt(result.resource); intent.current.clear();
    await client.invalidateQueries({ queryKey: ['releases', project] });
  }, onError: error => {
    const rejected = error instanceof ApiFailure && ((!!error.problem && error.status >= 400 && error.status < 500) || error.code === 'OFFLINE');
    if (!rejected) hadUnknown.current = true;
    if (rejected && !hadUnknown.current) setSubmitted(undefined);
  } });
  useGuard(!receipt);
  const retry = !!submitted && mutation.isError;
  function dismiss() {
    if (mutation.isPending) return;
    if (retry) modal.confirm({ title: '关闭尚未确认的冻结请求？', content: '关闭不会撤销可能已保存的目标包。请先核对交付记录，不要重复创建版本。', okText: '关闭并核对', cancelText: '保留原请求', onOk: close });
    else close();
  }
  function submit() {
    if (!online || mutation.isPending || receipt) return;
    if (submitted) { mutation.mutate(submitted); return; }
    if (!eligible) return;
    const body: Schema['ReleaseCreateV1'] = { schema_version: 1, candidate_id: candidate, evaluation_id: evaluation.id };
    setSubmitted(body); mutation.mutate(body);
  }
  if (detail && receipt) return <ReleaseDetail id={receipt.id} project={project} close={() => setDetail(false)} />;
  return <Modal open title="确认冻结目标包" width={720} maskClosable={false} closable={!mutation.isPending} onCancel={dismiss}
    footer={receipt ? <Button onClick={close}>返回候选</Button> : undefined} cancelText="返回" okText={retry ? '重试同一冻结请求' : '确认冻结 Release'}
    confirmLoading={mutation.isPending} okButtonProps={{ disabled: !online || (!retry && !eligible) }} onOk={submit}>
    <Space orientation="vertical" className="full-width" size="middle">
      <Alert showIcon type="info" title="冻结不会批准 Paper/Live，也不会发送目标给下游。" description="服务器重验原候选、独立评估、Alpha 资格、数据许可及有效期；权重、来源与截止时间不能在此改写。" />
      <Descriptions column={1} className="break-word" items={[
        { key: 'candidate', label: '原候选', children: candidate }, { key: 'evaluation', label: '原独立评估', children: evaluation.id },
        { key: 'state', label: '读取时评估', children: `${evaluation.execution_status} / ${evaluation.evidence_status} / ${evaluation.decision}` },
        { key: 'expiry', label: '原评估期限', children: evaluation.valid_until ? displayTime(evaluation.valid_until) : '无有效期' },
      ]} />
      <ErrorNotice error={mutation.error} />
      {!eligible && <Alert showIcon type="warning" title="原评估不满足冻结前提，请重新核对候选与独立评估。" />}
      {retry && <Alert showIcon type="warning" title="冻结结果尚未确认，重试保留原候选、评估与幂等键。" />}
      {receipt && <><Alert showIcon type="success" title="原目标包已冻结。" description="这不是交付审批或下游领取结果。" />
        <Typography.Text className="break-word">Release {receipt.id}</Typography.Text>
        <Button onClick={() => setDetail(true)}>查看原目标包</Button></>}
    </Space>
  </Modal>;
}
