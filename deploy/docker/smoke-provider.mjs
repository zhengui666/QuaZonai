// Test sidecar only. Never copied into an application image or release bundle.
// The pinned public pi-ai root import target (its exports are import-only).
import { fauxProvider, fauxAssistantMessage, fauxToolCall } from '/opt/quazonai/apps/service/node_modules/@earendil-works/pi-ai/dist/index.js';
const faux = fauxProvider({ tokensPerSecond: 100_000 });
export const model = { provider: faux.provider.id, modelId: faux.getModel().id };
faux.setResponses(process.env.QZ_TEST_IMAGE_SERVICE_ONLY === '1' ? [
  fauxAssistantMessage(fauxToolCall('research_finish', { reason: 'Service-only SQLite protocol, no native evidence' }), { stopReason: 'toolUse' }),
] : [
  fauxAssistantMessage(fauxToolCall('factor_evaluate', {
    hypothesis: 'Controlled image protocol; not alpha qualification',
    model: '(module (func (export "predict") (param f64 f64 f64 f64 f64 f64 f64 f64) (result f64) f64.const 1))',
  }), { stopReason: 'toolUse' }),
  (transcript) => {
    const result = transcript.messages.findLast((message) => message.role === 'toolResult' && message.toolName === 'factor_evaluate');
    const trial = JSON.parse(result.content.filter((item) => item.type === 'text').map((item) => item.text).join(''));
    if (!trial.trialId) throw new Error('image_native_trial_missing');
    return fauxAssistantMessage(fauxToolCall('factor_finalize', { trialId: trial.trialId, reason: 'Stop before heldout' }), { stopReason: 'toolUse' });
  },
]);
export default faux.provider;
