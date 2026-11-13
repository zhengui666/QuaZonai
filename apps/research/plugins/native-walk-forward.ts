import { defineDoc, type JsonObject, type TaskId } from '@earendil-works/pi-durable';

// Historical metadata only. QZ no longer generates folds or selects candidates.
export const NativeWalkForward = defineDoc<{ taskIds: TaskId<JsonObject>[] }>({
  kind: 'qz.native-walk-forward', version: 1, scope: 'conversation', history: 'latest', fork: 'initial',
  checkpointWhen: (_value, _ops, info) => info.deltasSinceBase >= 127, initial: () => ({ taskIds: [] }),
});
