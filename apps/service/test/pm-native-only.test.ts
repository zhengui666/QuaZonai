import { commands } from '../../nautilus/src/server.ts';
import { nativeCommands } from '../../research/src/native-execution.ts';
import { pmCapabilities } from '../src/pm-registry.ts';
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { configDefaults } from '../src/config.ts';
import { createPmPlatform } from '../src/pm-platform.ts';

test('removed custom PM research cannot create a campaign or invoke a scientific worker', async () => {
  // No host or native target is connected: removed research exits before either
  // can be touched, while the existing account-control surface stays available.
  const { work } = createPmPlatform(configDefaults({ jobBinary: '/must-not-execute' }));
  await assert.rejects(work.start({}), /pm_custom_research_removed/);
  await assert.rejects(work.startCampaign({}), /pm_custom_research_removed/);
  await assert.rejects(work.clarifyIntent('unused', {}), /pm_custom_research_removed/);
  for (const operation of [work.offerTarget, work.retire, work.refreshData, work.strategyCommand]) assert.equal(typeof operation, 'function');
});

test('PM capabilities do not advertise removed project-specific fitting and scoring', () => {
  const operations = pmCapabilities().flatMap(capability => capability.supportedOperations);
  for (const removed of ['fit-ensemble', 'freeze-ensemble-calibrations', 'capital-curve', 'study-portfolio', 'build-observed-spot', 'simulate', 'compare-observed-spot', 'value-observed-spot']) assert.equal(operations.includes(removed), false);
  assert.deepEqual(pmCapabilities(), []);
});

test('BAR capacity estimation and custom account sizing have no scientific execution entry', () => {
  for (const operation of ['assess-capacity', 'size-account']) {
    assert.equal(nativeCommands.has(operation), false);
    assert.equal((commands as readonly string[]).includes(operation), false);
  }
});
