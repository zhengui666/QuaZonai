import { Type } from '@earendil-works/pi-ai';
import { defineExtension, defineTool } from '@earendil-works/pi-durable';
import { realpath, stat } from 'node:fs/promises';
import { object, studyInput } from '../src/trials.ts';
import { iterationRequest } from './native-wasm.ts';
import { isTickResearchInput } from '../../../packages/contracts/src/tick-input.ts';

export default defineExtension({
  name: 'qz.data-catalog',
  tools: [defineTool({
    name: 'catalog_input', description: 'Describe the original configured native catalog and iterative time selection. Directory presence is not PIT, quality or permission evidence; raw held-out values are never returned.',
    parameters: Type.Object({}), replay: 'safe',
    async execute(_args, api, context) {
      const input = await studyInput(api, context);
      if (typeof input.data.catalog !== 'string') throw new Error('catalog_required');
      const catalog = await realpath(input.data.catalog);
      if (!(await stat(catalog)).isDirectory()) throw new Error('catalog_directory_required');
      const ensemble = input.data.catalog_ensemble_request !== undefined;
      const scan = ensemble || input.data.catalog_scan_request !== undefined;
      const tick = isTickResearchInput(input.data);
      const request = iterationRequest(input.data, ensemble);
      const selection = scan ? { scope: 'ALL_CATALOG_BAR_SERIES', ...request } : tick ? structuredClone(object(request.input, 'native_tick_input')) : structuredClone(object(object(request.forecast, 'forecast').selection, 'selection'));
      if (!ensemble) selection.event_end_ns = request.validation_end_ns!;
      return { content: [{ type: 'text', text: JSON.stringify({ catalog, datasetId: input.data.datasetId ?? null, selection, evidence: 'CONFIGURED_DIRECTORY_ONLY' }) }] };
    },
  })],
});
