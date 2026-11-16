import { lstat, realpath, stat } from 'node:fs/promises';
import { dirname, isAbsolute, relative, resolve, sep } from 'node:path';
import { defineExtension, hook, section, ToolTask } from '@earendil-works/pi-durable';
import { CodingTools } from '@earendil-works/pi-durable/tools';
import { NodeExecutionEnv } from '@earendil-works/pi-durable/env/node';

// cwd alone is not isolation. Only source files in the explicit workspace are
// exposed; unrestricted shell access would bypass sealed-data/account boundaries.
export async function codingWorkspace(directory: string) {
  if (!isAbsolute(directory)) throw new Error('research_coding_workspace_must_be_absolute');
  const root = await realpath(directory);
  if (!(await stat(root)).isDirectory()) throw new Error('research_coding_workspace_not_directory');
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
  return {
    env: () => new NodeExecutionEnv({ cwd: root }),
    extension: defineExtension({ name: 'qz.research-coding', tools,
      sections: [section('research-source', () => `You may create and edit Rust source only in the explicitly configured research source workspace ${root}, using official read/write/edit. No shell is exposed. Do not put credentials, market data or sealed/OOS data here. Use native_runner_preparation_status and the existing native_runner_build/build_status tools for the configured owner factory; writing source is not a build. After changing Cargo.toml dependencies, explicitly use prepareOwnerLock=true on native_runner_build to let official Cargo resolve Cargo.lock offline, and inspect preflight.log for changes. Never hand-edit Cargo.lock. Do not prepare the lock while another study edits or builds the same owner. Unknown build outcomes must not be automatically retried. Use native_research_start_bound after a verified build, without selecting a global runner or changing accounts. This workspace is shared by research conversations; preserve other research source and coordinate file changes. A configured compiler and owner project must actually be reachable from the service.`)],
      hooks: [hook(ToolTask, { async beforeTool(call) {
        if (!['read', 'write', 'edit'].includes(call.name)) return;
        if (typeof call.arguments.path !== 'string') return;
        return { arguments: { ...call.arguments, path: await sourcePath(call.arguments.path, call.name === 'write') } };
      } })],
    }),
  };
}
