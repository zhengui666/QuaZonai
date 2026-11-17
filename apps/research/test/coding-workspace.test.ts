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
    assert.ok(authoringPrompt.includes('任何兼容当前 Nautilus 原生能力的策略均可实现'));
    assert.ok(authoringPrompt.includes('不是唯一模板'));
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

test('independent synthesis reads only Task-declared reports with official line pagination', async () => {
  const { ResearchAssessmentTask } = await import('../plugins/research-assessment.ts');
  const { NativeStrategyBacktestTask } = await import('../plugins/native-strategy-backtest.ts');
  const directory = await mkdtemp(join(tmpdir(), 'qz-report-read-'));
  const workspace = join(directory, 'workspace'); await mkdir(workspace);
  const report = join(workspace, 'report.json'); await writeFile(report, 'FIRST_REPORT_LINE\nSECOND_REPORT_LINE\nLAST_REPORT_LINE\n');
  await writeFile(join(directory, 'outside.json'), 'OUTSIDE_SENTINEL');
  await symlink(join(directory, 'outside.json'), join(workspace, 'escape.json'));
  const faux = fauxProvider({ tokensPerSecond: 100000 }); const models = createModels(); models.setProvider(faux.provider);
  const host = await openResearch({ database: join(directory, 'research.sqlite'), codingWorkspace: workspace, models });
  try {
    const root = await host.harness.root(context);
    const native = async (path: string, conversation = root) => conversation.commit(tx => tx.createTask(NativeStrategyBacktestTask, { request: { name: 'test-actor', config: { report: path } }, catalog: directory, target: '/missing-report-reader-test-binary' }, { ownership: { kind: 'conversation' } }), context);
    const nativeId = await native(report); await host.harness.waitForTask(nativeId, context);
    const escapeId = await native(join(workspace, 'escape.json')); await host.harness.waitForTask(escapeId, context);
    const unlistedId = await native(report); await host.harness.waitForTask(unlistedId, context);
    const otherId = await root.commit(async tx => (await tx.createConversation({ ownership: { kind: 'ownerless' } })).id, context);
    const other = (await host.harness.conversation(otherId, context))!;
    const foreignId = await native(report, other); await host.harness.waitForTask(foreignId, context);
    faux.setResponses([
      call('native_assessment_read_report', { taskId: nativeId, offset: 1, limit: 1 }),
      input => { const wire = JSON.stringify(input.messages); assert.ok(wire.includes('FIRST_REPORT_LINE')); assert.ok(!wire.includes('SECOND_REPORT_LINE')); return call('native_assessment_read_report', { taskId: nativeId, offset: 2, limit: 2 }); },
      input => { assert.ok(JSON.stringify(input.messages).includes('LAST_REPORT_LINE')); return call('native_assessment_read_report', { taskId: escapeId }); },
      call('native_assessment_read_report', { taskId: unlistedId }),
      call('native_assessment_read_report', { taskId: foreignId }),
      input => { const wire = JSON.stringify(input.messages); assert.ok(!wire.includes('OUTSIDE_SENTINEL')); assert.ok(wire.includes('research_source_outside_workspace')); assert.ok(wire.includes('assessment_report_not_in_manifest')); assert.ok(wire.includes('assessment_report_task_unavailable')); return fauxAssistantMessage(JSON.stringify({ stars: null, rationale: 'Read current actor file, not immutable execution proof.', risks: [], uncertainties: [] })); },
    ]);
    const id = await root.commit(tx => tx.createTask(ResearchAssessmentTask, { mode: 'SYNTHESIS', model: { provider: faux.provider.id, modelId: faux.getModel().id }, request: { name: 'test-actor', config: { report } }, configurationTaskId: null, evidence: { trialHistory: { runs: [{ taskId: nativeId }, { taskId: escapeId }, { taskId: foreignId }] } } }, { ownership: { kind: 'conversation' } }), context);
    const task = await host.harness.waitForTask(id, context); assert.equal(task.state.outcome.status, 'completed');
    if (task.state.outcome.status !== 'completed') assert.fail('review failed');
    const childId = task.state.outcome.result.reviewConversationId as import('@earendil-works/pi-durable').ConversationId;
    const agent = await (await host.harness.conversation(childId, context))!.agent(context);
    assert.deepEqual(agent.tools.map(t => t.name).sort(), ['native_assessment_read_exposure', 'native_assessment_read_report']);
    assert.equal(await readFile(report, 'utf8'), 'FIRST_REPORT_LINE\nSECOND_REPORT_LINE\nLAST_REPORT_LINE\n');
  } finally { await host.close(); await rm(directory, { recursive: true, force: true }); }
});
