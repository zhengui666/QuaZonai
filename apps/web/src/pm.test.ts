import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { expect, it, vi } from 'vitest';
import Pm from './pm';
const { view, calls } = vi.hoisted(() => ({
  calls: [] as string[],
  view: {
    mandate: { id:'account-mandate', ownerGoal:'已有账户', revision:'7', baseCurrency:'USD', operatingAuthority:'AUTHORIZED_TARGETS', capitalSource:{ kind:'ACCOUNT', accountBindingId:'account/original' }, unresolvedIntent:['旧的未决研究问题'] },
    currentDecision:null, lastCycle:null, comparisons:[], strategies:[], assessments:[], campaigns:[], capacityEstimates:[], targetCommands:[], outcomes:[], history:[], gaps:[], ensembles:[],
  },
}));
// SSR only: command transport is exercised by CLI/API tests; do not issue writes here.
vi.mock('./commands', () => ({ useCommand: () => ({ busy:false, pending:undefined, error:undefined, submit:vi.fn() }) }));
vi.mock('./delete-record', () => ({ DeleteRecord: () => null }));
vi.mock('./data-hooks', () => ({ useResource: (path: string) => { calls.push(path); return {data:path==='/mandates'?[view]:view,isPending:false}; } }));
const render = (creating = false, id?: string) => {
  const client = new QueryClient();
  try { return renderToStaticMarkup(createElement(QueryClientProvider,{client},createElement(Pm,{route:{page:'pm',creating,id,params:new URLSearchParams()}}))); }
  finally { client.clear(); }
};
it('retires mandate creation without querying authoring data or showing a form', () => {
  calls.length=0;
  const html=render(true);
  expect(html).toContain('旧投资研究目标创建入口已移除');
  expect(html).toContain('#/research/new');
  expect(html).not.toContain('<form');
  expect(calls).toEqual([]);
  expect(render()).not.toContain('#/pm/new');
});
it('retains existing account capital exit and guarded target controls, not retired clarification writes', () => {
  const html=render(false,'account-mandate');
  expect(html).toContain('#/observations/account%2Foriginal?section=capital');
  expect(html).toContain('管理账户退出资金');
  expect(html).toContain('等待运营证据');
  expect(html).toContain('重算当前证据');
  expect(html).not.toContain('澄清投资意图');
  expect(html).not.toContain('intent-clarifications');
});
