import { defineExtension, defineTool } from '@earendil-works/pi-durable';
import { int, obj, opt, str } from '@earendil-works/pi-schema';

/** Service callbacks preserve the existing original Pi tasks and admission rules.
 * No source authoring, account operations or second execution scheduler. */
export function nativePreparationTools(operations: {
  configuration(): unknown;
  build(value: unknown): Promise<unknown>;
  read(requestId: string): Promise<unknown>;
  select(value: unknown): Promise<unknown>;
  startResearch(value: unknown): Promise<unknown>;
}) {
  const output = (value: unknown) => ({ output: [{ type: 'text' as const, text: JSON.stringify(value) }] });
  const request = obj({ requestId: str(), expectedRevision: int() });
  return defineExtension({ name: 'qz.native-preparation', tools: [
    defineTool({ name: 'native_runner_preparation_status', parameters: obj({}), replay: 'safe', executionMode: 'sequential',
      description: 'Read current configured Rust project identity, available factory modules, selected runner and settings revision. No inspection, compiler, code execution or account connection. Use this revision for an explicit build; missing environment cannot be invented.',
      async execute() { return output(operations.configuration()); } }),
    defineTool({ name: 'native_runner_build', parameters: request, replay: 'safe', executionMode: 'sequential',
      description: 'Explicitly build the already configured trusted Rust project using the existing Pi build task. Cargo executes trusted build code. Keep the same requestId for retries; UNKNOWN must not be retried as a new build. Does not write strategy source, select a runner or connect accounts. A returned running task is not build success. The original service-owned build can continue when this conversation ends; stopping the conversation is not proof the build stopped.',
      async execute(args) { return output(await operations.build(args)); } }),
    defineTool({ name: 'native_runner_build_status', parameters: obj({ requestId: str() }), replay: 'safe', executionMode: 'sequential',
      description: 'Read the original runner build task by requestId without building again. Original UNKNOWN and failure remain visible.',
      async execute(args) { return output(await operations.read(args.requestId)); } }),
    defineTool({ name: 'native_runner_select', parameters: request, replay: 'safe', executionMode: 'sequential',
      description: 'Explicitly select a successfully built runner as the global default for subsequent NEW research. Uses the original build revision CAS. Never changes existing research bindings or connects an account. Retain the returned revision for native_research_start_bound.',
      async execute(args) { return output(await operations.select(args)); } }),
    defineTool({ name: 'native_research_start_bound', parameters: obj({ requestId: str(), runnerBuildRequestId: str(), expectedRunnerRevision: int(), goal: str(), datasetId: str(), model: obj({ provider: str(), modelId: str() }), thinkingLevel: opt(str()), serviceTier: opt(str()) }), replay: 'safe', executionMode: 'sequential',
      description: 'Explicitly start a NEW research conversation with a completed, selected original build and registered dataset. Preserve requestId on retry. Existing research and UNKNOWN executions are not rebound or erased. Uses normal model/dataset/admission checks, never starts live trading. Returns original conversation/submission references, not a completed research result.',
      async execute(args) { return output(await operations.startResearch(args)); } }),
  ] });
}
