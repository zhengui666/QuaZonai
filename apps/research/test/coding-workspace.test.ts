import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, mkdir, readFile, rm, symlink, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels, fauxAssistantMessage, fauxProvider, fauxToolCall } from '@earendil-works/pi-ai';
import type { JsonObject } from '@earendil-works/pi-durable';
import { openResearch } from '../src/host.ts';

const context = BACKGROUND_CONTEXT;
const call = (name: string, args: JsonObject) => fauxAssistantMessage(fauxToolCall(name, args), { stopReason: 'toolUse' });
test('official coding tools author and edit Rust in an explicit workspace, preserve files on reopen and deny outside access', async () => {
  const root = await mkdtemp(join(tmpdir(), 'qz-source-tools-'));
  const workspace = join(root, 'source'); await mkdir(workspace);
  await writeFile(join(root, 'sealed.txt'), 'SEALED_NOT_MODEL_INPUT');
  await symlink(root, join(workspace, 'escape'));
  await symlink(join(root, 'not-created.txt'), join(workspace, 'dangling'));
  await symlink(join(root, 'not-created-directory'), join(workspace, 'dangling-dir'));
  const faux = fauxProvider(); const models = createModels(); models.setProvider(faux.provider);
  const options = { database: join(root, 'research.sqlite'), codingWorkspace: workspace, models };
  let host = await openResearch(options);
  try {
    let authoringPrompt = "";
    faux.setResponses([
      transcript => {
        authoringPrompt = JSON.stringify(transcript);
        return call('write', { path: 'src/lib.rs', content: 'pub const VALUE: u32 = 1;\n' });
      },
      call('edit', { path: 'src/lib.rs', edits: [{ oldText: '= 1;', newText: '= 2;' }] }),
      call('read', { path: 'src/lib.rs' }),
      call('read', { path: '../sealed.txt' }),
      call('write', { path: 'escape/new.txt', content: 'must not write' }),
      call('write', { path: 'dangling', content: 'must not write' }),
      call('write', { path: 'dangling-dir/new.rs', content: 'must not write' }),
      fauxAssistantMessage('Source authored; no compile or backtest claimed.'),
    ]);
    const receipt = await host.start('author', { goal: 'Author research source', model: { provider: faux.provider.id, modelId: faux.getModel().id }, data: {}, policy: {} });
    await (await host.harness.submission(receipt.submissionId, context))!.wait(context);
    assert.equal(await readFile(join(workspace, 'src/lib.rs'), 'utf8'), 'pub const VALUE: u32 = 2;\n');
    assert.ok(authoringPrompt.includes('Rust 策略编写合同'));
    assert.ok(authoringPrompt.includes('pub fn config(catalog: &Path'));
    assert.ok(authoringPrompt.includes('nautilus_strategy!(CallbackCheck)'));
    assert.ok(authoringPrompt.includes('native_configured_configure'));
    assert.ok(authoringPrompt.includes('special_event_observation'));
    await assert.rejects(readFile(join(root, 'new.txt')), { code: 'ENOENT' });
    await assert.rejects(readFile(join(root, 'not-created.txt')), { code: 'ENOENT' });
    await assert.rejects(readFile(join(root, 'not-created-directory/new.rs')), { code: 'ENOENT' });
    const agent = await (await host.harness.conversation(receipt.conversationId, context))!.agent(context);
    const visible = agent.tools.map(tool => tool.name);
    assert.ok(visible.includes('write')); assert.ok(visible.includes('edit')); assert.ok(visible.includes('read')); assert.ok(!visible.includes('bash'));
    const view = JSON.stringify(await host.view(receipt.conversationId));
    assert.ok(view.includes('research_source_outside_workspace')); assert.ok(!view.includes('SEALED_NOT_MODEL_INPUT'));
    await host.close(); host = await openResearch(options);
    assert.equal(await readFile(join(workspace, 'src/lib.rs'), 'utf8'), 'pub const VALUE: u32 = 2;\n');
    assert.deepEqual(await host.start('author', { goal: 'Author research source', model: { provider: faux.provider.id, modelId: faux.getModel().id }, data: {}, policy: {} }), receipt);
  } finally { await host.close(); await rm(root, { recursive: true, force: true }); }
});


test('coding tools are absent by default and from literature-only conversations', async () => {
  const root = await mkdtemp(join(tmpdir(), 'qz-source-disabled-'));
  const faux = fauxProvider(); const models = createModels(); models.setProvider(faux.provider);
  try {
    for (const enabled of [false, true]) {
      const host = await openResearch({ database: join(root, `research-${enabled}.sqlite`), models, ...(enabled ? { codingWorkspace: root } : {}) });
      try {
        faux.setResponses([fauxAssistantMessage('Discussion only.')]);
        const receipt = await host.start('discussion', { goal: 'Discuss only', model: { provider: faux.provider.id, modelId: faux.getModel().id }, data: enabled ? { dataset: { id: 'discussion' } } : {}, policy: {}, ...(enabled ? { mode: 'LITERATURE_ONLY' as const } : {}) });
        await (await host.harness.submission(receipt.submissionId, context))!.wait(context);
        const agent = await (await host.harness.conversation(receipt.conversationId, context))!.agent(context);
        assert.ok(!agent.tools.some(tool => ['read', 'write', 'edit', 'bash'].includes(tool.name)));
      } finally { await host.close(); }
    }
  } finally { await rm(root, { recursive: true, force: true }); }
});
