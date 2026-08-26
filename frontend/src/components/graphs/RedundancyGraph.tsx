import { Background, Controls, MarkerType, ReactFlow, type Edge, type Node } from '@xyflow/react';
import '@xyflow/react/dist/style.css';
import { useI18n, type Locale } from '../../i18n';
import { useReactFlowAriaLabelConfig } from './reactFlowA11y';
import type { CandidateMember } from '../../lib/api/types';
import { humanize } from '../../lib/format';

export interface RedundancyEdge { source: string; target: string; score?: number; reason?: string; }

export function formatRedundancyScore(locale: Locale, score: number): string {
  return new Intl.NumberFormat(locale, { maximumSignificantDigits: 15 }).format(score);
}

export function RedundancyGraph({ members, edges }: { members: CandidateMember[]; edges?: RedundancyEdge[] }) {
  const { locale } = useI18n();
  const ariaLabelConfig = useReactFlowAriaLabelConfig();

  const nodes: Node[] = members.map((member, index) => ({ id: member.alpha_qualification_id, position: { x: (index % 4) * 210, y: Math.floor(index / 4) * 125 }, data: { label: <span>{member.alpha_name ? <bdi dir="auto">{member.alpha_name}</bdi> : <bdi dir="ltr">{member.alpha_qualification_id.slice(0, 8)}</bdi>} <span dir="auto">· {humanize(member.role)}</span></span> }, style: { background: 'var(--qz-bg-elevated)', border: '1px solid var(--qz-border-strong)', color: 'var(--qz-text)', borderRadius: 8, width: 175, fontSize: 10 } }));
  const flowEdges: Edge[] = (edges ?? []).map((edge, index) => {
    const label = edge.score !== undefined ? formatRedundancyScore(locale, edge.score) : edge.reason;
    return { id: `r-${index}`, source: edge.source, target: edge.target, label: label ? <bdi dir="auto">{label}</bdi> : undefined, markerEnd: { type: MarkerType.ArrowClosed }, style: { stroke: 'var(--qz-warning)' }, labelStyle: { fill: 'var(--qz-text-faint)', fontSize: 9 } };
  });
  return <div className="qz-flow"><ReactFlow ariaLabelConfig={ariaLabelConfig} nodes={nodes} edges={flowEdges} fitView nodesDraggable={false} nodesConnectable={false} minZoom={.4}><Background gap={22} color="var(--qz-border)" /><Controls showInteractive={false} /></ReactFlow></div>;
}
