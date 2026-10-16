import { Type } from '@earendil-works/pi-ai';
import { defineExtension, defineTool } from '@earendil-works/pi-durable';
import type { JsonObject } from '@earendil-works/pi-durable';
import { Study } from '../src/documents.ts';
import { object } from '../src/trials.ts';

export default defineExtension({
  name: 'qz.validation-compare',
  tools: [defineTool({
    name: 'validation_compare', description: 'Additional evaluator plugin: compare all disclosed trials by the originally requested validation Pearson IC. Missing metrics are ineligible, never zero. This is selection feedback, not qualification.',
    parameters: Type.Object({}), replay: 'safe',
    async execute(_args, api, context) {
      const study = await api.snapshot(Study, api.conversationId, context);
      if (study?.input?.policy.selection_metric !== 'validation.pearson_ic') throw new Error('selection_metric_unsupported');
      if (study.final !== null) throw new Error('research_already_final');
      const compared = study.trials.map((trial) => {
        const result = trial.result as JsonObject | null;
        const partitions = result?.partitions as JsonObject[] | undefined;
        const validation = partitions?.find((item) => item.partition === 'VALIDATION');
        const ic = validation ? object(object(validation.metric, 'metric').pearson_ic, 'pearson_ic') : null;
        return { trialId: trial.taskId!, trialStatus: trial.status!, metricStatus: ic?.status ?? 'UNAVAILABLE', value: ic?.value ?? null };
      });
      const eligible = compared.filter((item) => item.metricStatus === 'AVAILABLE' && typeof item.value === 'number');
      eligible.sort((left, right) => (right.value as number) - (left.value as number));
      return { content: [{ type: 'text', text: JSON.stringify({ compared, selectedTrialId: eligible[0]?.trialId ?? null, criterion: study.input.policy.selection_metric }) }] };
    },
  })],
});
