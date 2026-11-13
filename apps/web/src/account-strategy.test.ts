import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { expect, it, vi } from 'vitest';
import type { ObservationView } from '../../../packages/contracts/src/index.ts';
import AccountStrategy, { strategyResultText } from './account-strategy';

const mocks=vi.hoisted(()=>({kind:'EMA_CROSS',submits:0,commands:[] as string[]}));
vi.mock('./commands',()=>({readDraft:()=>undefined,saveDraft:()=>true,useCommand:(kind:string,_accepted:unknown,path:string)=>{mocks.commands.push(`${kind}:${path}`);return {pending:undefined,busy:false,submit:()=>{mocks.submits++;},values:undefined};}}));
vi.mock('./ui',()=>({useConfirm:()=>()=>{},ErrorNotice:()=>null,PendingCommand:()=>null,Status:({value}:{value:string})=>value,Facts:()=>null}));
vi.mock('./data-hooks',()=>({useResource:(path:string)=>({refetch:()=>{},data:path.endsWith('/fence')?{accountSourceId:'source-original',strategyRevision:'7'}:path.endsWith('/strategy')?{accountSourceId:'source-original',strategyRevision:'7',strategy:{strategy_id:'EMA_CROSS-001',state:'STOPPED',instrument_id:'BTC-USDC-SPOT.HYPERLIQUID',native_version:'0.69.0',config:{kind:mocks.kind,trade_size:'0.000000000000000001',fast_period:2,slow_period:3}}}:undefined})}));
const value={id:'account/a',mode:'live',connectionName:'Explicit account'} as ObservationView;
it('exposes real HTTP strategy command hooks separately from account observation and funds',()=>{
  mocks.commands=[];mocks.kind='EMA_CROSS';const html=renderToStaticMarkup(createElement(AccountStrategy,{value}));
  expect(mocks.commands).toContain('strategy:account/a:START:/observations/account%2Fa/strategy/commands');
  expect(mocks.commands).toContain('strategy:account/a:STOP:/observations/account%2Fa/strategy/commands');
  expect(mocks.commands).toContain('strategy:account/a:RECOVER:/observations/account%2Fa/strategy/commands');
  expect(mocks.submits).toBe(0); expect(html).toContain('连接账户进行恢复'); expect(html).toContain('不会自动连接');
  expect(html).toContain('启动实盘策略');expect(html).toContain('停止策略');expect(html).toContain('0.000000000000000001');
  expect(html).toContain('REGISTERED');expect(html).toContain('仅保存配置');expect(html).toContain('不保证撤销全部订单');
});
it('distinguishes confirmed STOP without durable receipt from a missing native reply',()=>{
 const base={requestId:'original',action:'STOP' as const,status:'UNCERTAIN' as const,error:null};
 expect(strategyResultText({...base,receipt:{persisted:false,result:{state:'STOPPED'}}})).toContain('原生已确认停止');
 expect(strategyResultText({...base,receipt:{persisted:false,result:null}})).toContain('未确认停止');
 expect(strategyResultText({...base,receipt:null})).toContain('不自动重发');
 expect(strategyResultText({...base,action:'START',status:'REJECTED',error:'strategy_start_fence_changed',receipt:null})).toContain('不会自动换版本重试');
});

it('does not equate a recovery connection receipt with reconciliation or trading permission',()=>{
 expect(strategyResultText({requestId:'recovery',action:'RECOVER',status:'COMPLETED',receipt:{result:{recovery_only:true}},error:null})).toContain('不代表订单已对账');
 expect(strategyResultText({requestId:'recovery',action:'RECOVER',status:'UNCERTAIN',receipt:null,error:null})).toContain('不自动重连');
});
