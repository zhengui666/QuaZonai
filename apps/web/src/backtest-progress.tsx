import type { PortfolioView } from '../../../packages/contracts/src/index.ts';
import { Timestamp } from './ui';

const phases: Record<string, string> = { QUEUED: '等待回测', FORECASTING: '生成成员预测', SIMULATING: '原生账户回测', CAPACITY: '评估组合容量', COMPLETED: '回测完成', FAILED: '回测失败', STOPPED: '回测已停止' };
export function BacktestProgress({ portfolio, compact = false }: { portfolio: PortfolioView; compact?: boolean }) {
  const active = !['COMPUTED', 'FAILED', 'STOPPED'].includes(portfolio.status);
  const progress = portfolio.progress;
  if (!active && compact || !active && !progress) return null;
  const label = progress ? phases[progress.phase] ?? progress.phase : portfolio.status === 'QUEUED' ? '等待回测' : '等待回测进度';
  const counts = progress?.completed !== undefined && progress.total !== undefined ? `（${progress.completed} / ${progress.total}）` : '';
  if (compact) return <div className="quiet" role="status">{label}{counts}</div>;
  return <section className="result-section" aria-label="回测进度"><h3>回测进度</h3><p role="status">{label}{counts}</p>{progress && <p className="quiet">更新于 <Timestamp value={progress.updatedAt} /></p>}{active && <p className="quiet">{portfolio.curve.length ? '回测未结束' : '等待首个权益快照'}</p>}</section>;
}
