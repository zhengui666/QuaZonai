import { useTranslation } from 'react-i18next';
import { tr } from './i18n';
import type { ResearchSpecification } from '../../../packages/contracts/src/index.ts';
import { Facts, MetricLabel } from './ui';
import { Disclosure } from './disclosure';
export function Mandate({ specification: spec }: { specification: ResearchSpecification }) {
  useTranslation('core');
  return <section className="mandate-section"><h3>{tr("Research brief")}</h3><Facts items={[[tr("Evaluation"), <><MetricLabel metricKey={spec.evaluation.metric} />  {tr("/ maximize /")} {spec.evaluation.target ?? tr("No threshold set")}</>], [tr("Search scope"), spec.search.scope === 'single-factor' ? tr("Single Wasm factor") : tr("Native shared-account portfolio / up to {{count}} models", { count: spec.search.maxMembers })]]} />
    <Disclosure title={tr("Research question, evidence, and original objective")} className="text-disclosure"><Facts items={[[tr("Economic question"), spec.question], [tr("Hypothesis and evidence"), spec.rationale], ...(spec.exclusions ? [[tr("Exclusions"), spec.exclusions] as [string, string]] : []), [tr("Stopping conditions"), tr("Final selection")]]} /></Disclosure>
    {spec.capital && <Disclosure title={tr("Original research capital assumptions")}><Facts items={[[tr("Capital scenario"), spec.capital.scenarios.join(' / ')], [tr("Volume participation rate"), spec.capital.participation], [tr("Maximum liquidity data lag"), tr("{{seconds}} seconds", { seconds: spec.capital.maximumAgeSeconds })]]} /></Disclosure>}
  </section>;
}
