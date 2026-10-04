import type { Schema } from './api';
import { CandidateDetail } from './portfolio-candidates';

// Preserve navigation to the original candidate without submitting a new Study.
export function PortfolioStudy({ candidate, close }: { candidate: Schema['CandidateViewV1']; close: () => void }) {
  return <CandidateDetail id={candidate.id} project={candidate.project_id} close={close} />;
}
