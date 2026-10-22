import { Descriptions, Typography } from 'antd';
import type { ReactNode } from 'react';
import { readableMessage } from './readable-message';

const commonLabels: Record<string, string> = {
  id: '记录编号', project_id: '项目编号', schema_version: '格式版本', revision: '版本',
  created_at: '创建时间', updated_at: '更新时间', authorized_at: '授权时间',
  content: '配置内容', version: '版本', parameters: '参数', settings: '设置',
  adapter_kind: '适配器', upstream_class: '原生模型', upstream_version: '原生版本',
  metric_code: '指标', scope: '范围', comparator: '比较方式', threshold_low: '下限',
  threshold_high: '上限', minimum_observations: '最少观察数', method_allowlist: '允许的方法',
  required: '必需', name: '名称', kind: '类型', enabled: '启用', status: '状态',
  value: '值', description: '说明', message: '提示', detail: '详情', code: '代码',
};
const unreadable = '此内容暂时无法整理为完整字段。';

// Preserve numeric lexemes before parsing historical text. JSON.parse alone
// rounds large integers and precise decimals, even though this is a read view.
function parseStructured(text: string): unknown {
  JSON.parse(text); // Validate before quoting tokens; invalid numbers must stay invalid.
  const quotedNumbers = text.replace(/"(?:\\[\s\S]|[^"\\])*"|(-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?)/g,
    (token, number: string | undefined) => number === undefined ? token : `"${number}"`);
  return JSON.parse(quotedNumbers);
}

function decodeText(text: string): { structured: boolean; value: unknown } {
  const trimmed = text.trim();
  const fence = /^```(?:json|jsonl|ndjson)?\s*\n([\s\S]*?)\n```$/i.exec(trimmed);
  const candidate = fence?.[1]?.trim() ?? trimmed;
  if (!/^(?:\{\s*(?:"|\}|$)|\[\s*(?:"|\{|\[|-?\d|true\b|false\b|null\b|\]|$)|")/.test(candidate)) return { structured: false, value: text };
  try { return { structured: true, value: parseStructured(candidate) }; }
  catch {
    if (candidate.startsWith('"')) return { structured: false, value: text };
    const lines = candidate.split(/\r?\n/).filter(line => line.trim());
    if (lines.length > 1) {
      try { return { structured: true, value: lines.map(line => parseStructured(line)) }; } catch { /* Incomplete content must not fall back to raw syntax. */ }
    }
    return { structured: true, value: undefined };
  }
}

/** A complete read view of saved data, including serialized historical fields. */
export function DataDetails({ value, label, labels = {} }: {
  value: unknown; label?: string; labels?: Record<string, string>;
}) {
  function render(current: unknown, depth: number): ReactNode {
    if (depth > 24) return <Typography.Text type="secondary">内容层级过深，无法展开更多字段。</Typography.Text>;
    if (typeof current === 'string') {
      const decoded = decodeText(current);
      if (decoded.structured) {
        if (decoded.value === undefined) return <Typography.Text type="secondary">{unreadable}</Typography.Text>;
        return render(decoded.value, depth + 1);
      }
      return <Typography.Text style={{ whiteSpace: 'pre-wrap' }}>{current === '' ? '空文本' : readableMessage(current)}</Typography.Text>;
    }
    if (current === null) return <Typography.Text type="secondary">无值</Typography.Text>;
    if (current === undefined) return <Typography.Text type="secondary">未提供</Typography.Text>;
    if (typeof current === 'boolean') return current ? '是' : '否';
    if (typeof current === 'number' || typeof current === 'bigint') return String(current);
    if (Array.isArray(current)) {
      if (current.length === 0) return <Typography.Text type="secondary">无条目</Typography.Text>;
      return <ol className="data-details-list">{current.map((item, index) => <li key={index}>{render(item, depth + 1)}</li>)}</ol>;
    }
    if (typeof current === 'object') {
      const entries = Object.entries(current);
      if (!entries.length) return <Typography.Text type="secondary">无字段</Typography.Text>;
      return <Descriptions size="small" column={1} layout="vertical" items={entries.map(([key, item]) => ({
        key, label: Object.hasOwn(labels, key) ? labels[key] : Object.hasOwn(commonLabels, key) ? commonLabels[key] : readableMessage(key), children: render(item, depth + 1),
      }))} />;
    }
    return <Typography.Text type="secondary">无法显示此内容</Typography.Text>;
  }
  return <div role={label ? 'region' : undefined} aria-label={label} className="data-details break-word">{render(value, 0)}</div>;
}
