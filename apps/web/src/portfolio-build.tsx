import type { Schema } from './api';
import { MandateDetail } from './portfolio';

// Historical entrypoints remain readable; business commands belong to the external CLI/Skill.
export function PortfolioBuild({ mandate, close }: { mandate: Schema['MandateViewV1']; close: () => void }) {
  return <MandateDetail id={mandate.id} project={mandate.project_id} close={close} />;
}
