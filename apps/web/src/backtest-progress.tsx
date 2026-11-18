import { useTranslation } from 'react-i18next';
import { tr } from './i18n';
import type { PortfolioView } from '../../../packages/contracts/src/index.ts';
import { Timestamp } from './ui';

const phases: Record<string, string> = { QUEUED: "Awaiting backtest", FORECASTING: "Generating member predictions", SIMULATING: "Native account backtest", CAPACITY: "Assessing portfolio capacity", COMPLETED: "Backtest completed", FAILED: "Backtest failed", STOPPED: "Backtest stopped" };
export function BacktestProgress({ portfolio, compact = false }: { portfolio: PortfolioView; compact?: boolean }) {
  useTranslation('core');
  const active = !['COMPUTED', 'FAILED', 'STOPPED'].includes(portfolio.status);
  const progress = portfolio.progress;
  if (!active && compact || !active && !progress) return null;
  const label = progress ? phases[progress.phase] ? tr(phases[progress.phase]!) : progress.phase : portfolio.status === 'QUEUED' ? tr("Awaiting backtest") : tr("Awaiting backtest progress");
  const counts = progress?.completed !== undefined && progress.total !== undefined ? `（${progress.completed} / ${progress.total}）` : '';
  if (compact) return <div className="quiet" role="status">{label}{counts}</div>;
  return <section className="result-section" aria-label={tr("Backtest progress")}><h3>{tr("Backtest progress")}</h3><p role="status">{label}{counts}</p>{progress && <p className="quiet">{tr("Updated")} <Timestamp value={progress.updatedAt} /></p>}{active && <p className="quiet">{portfolio.curve.length ? tr("Backtest is still in progress") : tr("Awaiting the first equity snapshot")}</p>}</section>;
}
