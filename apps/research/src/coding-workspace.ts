import { lstat, readFile, realpath, stat } from 'node:fs/promises';
import { dirname, isAbsolute, relative, resolve, sep } from 'node:path';
import { defineExtension, defineTool, hook, section, ToolTask, type JsonObject, type TaskId } from '@earendil-works/pi-durable';
import { int, obj, opt } from '@earendil-works/pi-schema';
import { CodingTools, createReadTool } from '@earendil-works/pi-durable/tools';
import { NodeExecutionEnv } from '@earendil-works/pi-durable/env/node';

// cwd alone is not isolation. Only source files in the explicit workspace are
// exposed; unrestricted shell access would bypass sealed-data/account boundaries.
export async function codingWorkspace(directory: string) {
  if (!isAbsolute(directory)) throw new Error('research_coding_workspace_must_be_absolute');
  const root = await realpath(directory);
  if (!(await stat(root)).isDirectory()) throw new Error('research_coding_workspace_not_directory');
  const authoring = await readFile(new URL('../skills/strategy-authoring/SKILL.md', import.meta.url), 'utf8');
  const chartGuide = await readFile(new URL('../skills/strategy-authoring/references/native-result-charts.md', import.meta.url), 'utf8');
  const example = await readFile(new URL('../skills/strategy-authoring/assets/research.rs', import.meta.url), 'utf8');
  const tools = CodingTools.tools!.filter(tool => ['read', 'write', 'edit'].includes(tool.name));
  async function sourcePath(path: string, writing: boolean): Promise<string> {
    // Keep paths literal, avoiding the upstream convenience aliases/normalizations.
    if (!path || path.startsWith('~') || path.startsWith('@') || path.startsWith('file:') || /[\u00a0\u2000-\u200a\u202f\u205f\u3000]/u.test(path)) throw new Error('research_source_path_must_be_literal');
    const requested = resolve(root, path);
    let existing = requested;
    while (true) {
      try {
        const canonical = await realpath(existing);
        const result = resolve(canonical, relative(existing, requested));
        const within = relative(root, result);
        if (!within || within === '..' || within.startsWith(`..${sep}`) || isAbsolute(within)) throw new Error('research_source_outside_workspace');
        return result;
      } catch (error) {
        if (!writing || (error as NodeJS.ErrnoException).code !== 'ENOENT' || dirname(existing) === existing) throw error;
        // ENOENT may be a dangling symlink, not a new file. Never let the
        // upstream writer follow that link to create a target outside the root.
        try { await lstat(existing); throw new Error('research_source_unresolved_symlink'); }
        catch (missing) { if ((missing as NodeJS.ErrnoException).code !== 'ENOENT') throw missing; }
        existing = dirname(existing);
      }
    }
  }
  const read = createReadTool();
  const reportRead = defineTool({
    name: 'native_assessment_read_report',
    description: 'Read the report file declared by an original native Task in this independent assessment manifest. Uses official Pi read: offset is a 1-based line number and limit is a line count; follow its continuation/truncation diagnostics. This is the current file at the Task-declared path, not an immutable Task result or proof of its producer. Disclose actual ranges and incomplete reads. No backtest, write, shell or arbitrary path access.',
    parameters: obj({ taskId: int({ minimum: 1 }), offset: opt(int({ minimum: 1 })), limit: opt(int({ minimum: 1 })) }),
    async execute(args, api, context) {
      const declared = await api.commit(async tx => {
        const conversation = await tx.conversation(api.conversationId);
        const owner = conversation?.owner ? await tx.task(conversation.owner.taskId) : null;
        const input = owner?.input as JsonObject | undefined;
        if (owner?.kind !== 'qz.research.assessment' || input?.mode !== 'SYNTHESIS') throw new Error('assessment_report_review_only');
        const runs = ((input.evidence as JsonObject).trialHistory as JsonObject)?.runs;
        if (!Array.isArray(runs) || !runs.some(run => (run as JsonObject)?.taskId === args.taskId)) throw new Error('assessment_report_not_in_manifest');
        const task = await tx.task(args.taskId as TaskId);
        if (task?.conversationId !== owner.conversationId || task.kind !== 'qz.native-strategy.backtest') throw new Error('assessment_report_task_unavailable');
        const request = (task.input as JsonObject).request as JsonObject;
        const report = (request.config as JsonObject | undefined)?.report;
        if (typeof report !== 'string') throw new Error('assessment_report_path_missing');
        return report;
      }, context);
      const path = await sourcePath(declared, false);
      return read.execute({ path, ...(args.offset === undefined ? {} : { offset: args.offset }), ...(args.limit === undefined ? {} : { limit: args.limit }) }, api, context);
    },
  });
  return {
    reportExtension: defineExtension({ name: 'qz.research-report-read', tools: [reportRead] }),
    env: () => new NodeExecutionEnv({ cwd: root }),
    extension: defineExtension({ name: 'qz.research-coding', tools,
      sections: [section('native-result-presentation', () => chartGuide), section('research-source', () => `You may create and edit Rust source only in the explicitly configured research source workspace ${root}, using official read/write/edit. No shell is exposed. Do not put credentials, market data or sealed/OOS data here. Use native_runner_preparation_status and the existing native_runner_build/build_status tools for the configured owner factory; writing source is not a build. After changing Cargo.toml dependencies, explicitly use prepareOwnerLock=true on native_runner_build to let official Cargo resolve Cargo.lock offline, and inspect preflight.log for changes. Never hand-edit Cargo.lock. Do not prepare the lock while another study edits or builds the same owner. Unknown build outcomes must not be automatically retried. Use native_research_start_bound after a verified build, without selecting a global runner or changing accounts. This workspace is shared by research conversations; preserve other research source and coordinate file changes. A configured compiler and owner project must actually be reachable from the service.\n\n${authoring}\n\nComplete owner module (assets/research.rs):\n\`\`\`rust\n${example}\n\`\`\``)],
      hooks: [hook(ToolTask, { async beforeTool(call) {
        if (!['read', 'write', 'edit'].includes(call.name)) return;
        if (typeof call.arguments.path !== 'string') return;
        return { arguments: { ...call.arguments, path: await sourcePath(call.arguments.path, call.name === 'write') } };
      } })],
    }),
  };
}
