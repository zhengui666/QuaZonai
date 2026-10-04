import type { Schema } from './api';
import { EvaluationDetail } from './alphas';

// A legacy link may show the original evidence, but cannot freeze another target package.
export function ReleaseCreate({ project, candidate, evaluation, close }: {
  project: string; candidate: string; evaluation: Schema['EvaluationView']; close: () => void;
}) {
  return <EvaluationDetail id={evaluation.id} candidate={{ id: candidate, project }} close={close} />;
}
