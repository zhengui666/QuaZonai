import { defineDoc } from '@earendil-works/pi-durable';
import type { LiteratureView } from '../../../packages/contracts/src/literature.ts';

// Retrieved references and attributed model claims are research facts, not
// scientific results or a second copy of the native task/conversation state.
export const LiteratureDoc = defineDoc<LiteratureView>({
  kind: 'qz.literature', version: 1, scope: 'conversation', history: 'rewindable',
  fork: 'current', initial: () => ({ searches: [], notes: [] }),
});
