import { researchInput } from '../../research/src/input.ts';
import { object } from '../src/facts.ts';
import type { Service } from '../src/service.ts';

/** Test-only controlled producers use the public official intake directly. */
export function preserveLegacyPmAdmission(service: Service) {
  service.work.startResearch = async value => {
    const body = object(value);
    return service.research.start(String(body.requestId), researchInput(body.input), undefined, undefined, { original: body });
  };
}
