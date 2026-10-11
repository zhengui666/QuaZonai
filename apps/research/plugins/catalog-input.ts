import { Type } from '@earendil-works/pi-ai';
import { defineExtension, defineTool } from '@earendil-works/pi-durable';
import { realpath, stat } from 'node:fs/promises';
import { object, studyInput } from '../src/trials.ts';

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
      const request = object(input.data.request, 'native_request');
      const selection = structuredClone(object(object(request.forecast, 'forecast').selection, 'selection'));
      selection.event_end_ns = request.validation_end_ns!;
      return { content: [{ type: 'text', text: JSON.stringify({ catalog, datasetId: input.data.datasetId ?? null, selection, evidence: 'CONFIGURED_DIRECTORY_ONLY' }) }] };
    },
  })],
});
