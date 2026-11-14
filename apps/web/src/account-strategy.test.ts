import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { expect, it, vi } from 'vitest';
import type { ObservationView } from '../../../packages/contracts/src/index.ts';
import AccountStrategy, { strategyResultText, StrategyLifecycleNotice, AccountObservationNotice, LegacyAccountObservation, accountSummaryStartAllowed } from './account-strategy';

const mocks=vi.hoisted(()=>({kind:'EMA_CROSS',submits:0,commands:[] as string[],observation:undefined as undefined|{supported:boolean|null;nativeSessionId?:string}}));
vi.mock('./commands',()=>({readDraft:()=>undefined,saveDraft:()=>true,useCommand:(kind:string,_accepted:unknown,path:string)=>{mocks.commands.push(`${kind}:${path}`);return {pending:undefined,busy:false,submit:()=>{mocks.submits++;},values:undefined};}}));
vi.mock('./ui',()=>({useConfirm:()=>()=>{},ErrorNotice:()=>null,PendingCommand:()=>null,Status:({value}:{value:string})=>value,Facts:()=>null}));
vi.mock('./data-hooks',()=>({useResource:(path:string)=>({refetch:()=>{},data:path.endsWith('/config')?{accountSourceId:'source-original',strategyRevision:'7',configuration:{instrument_id:'BTC-USDC-SPOT.HYPERLIQUID',strategy:{kind:'EMA_CROSS',trade_size:'1',fast_period:2,slow_period:3}},connected:true,accountObservation:mocks.observation,pendingConfigurationRequests:[]}:path.endsWith('/fence')?{accountSourceId:'source-original',strategyRevision:'7'}:path.endsWith('/strategy')?{accountSourceId:'source-original',strategyRevision:'7',strategy:{strategy_id:'EMA_CROSS-001',state:'STOPPED',instrument_id:'BTC-USDC-SPOT.HYPERLIQUID',native_version:'0.69.0',config:{kind:mocks.kind,trade_size:'0.000000000000000001',fast_period:2,slow_period:3}}}:undefined})}));
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

it('shows the temporary live gate without claiming a strategy started or hiding paper START',()=>{
 const html=renderToStaticMarkup(createElement(AccountStrategy,{value}));
 expect(html).toContain('实盘启动暂不可用');
 expect(html).toMatch(/<button[^>]*disabled[^>]*>[\s\S]*?启动实盘策略/);
 const paper=renderToStaticMarkup(createElement(AccountStrategy,{value:{...value,mode:'paper'}}));
 expect(paper).not.toContain('实盘启动暂不可用');
 expect(strategyResultText({requestId:'live',action:'START',status:'REJECTED',error:'hyperliquid_native_spot_free_required',receipt:null})).toContain('本次未连接或启动策略');
});

it('shows only observed current-session lifecycle facts, never treating declaration as certification',()=>{
 const fact={supported:true,configuredKind:'REGISTERED',scope:'INSTALLED_DISPATCH_ONLY',strategyVerified:false,nativeSessionId:'original-session'};
 const html=renderToStaticMarkup(createElement(StrategyLifecycleNotice,{value:fact}));
 expect(html).toContain('最近一次读取到的原生会话报告');expect(html).toContain('缓存报告可能过时');expect(html).toContain('后台暂停轮询');expect(html).toContain('不证明会话持续在线');expect(html).toContain('已安装此策略的派发入口');expect(html).toContain('original-session');expect(html).toContain('不是认证');
 for(const value of [undefined,{...fact,nativeSessionId:null},{...fact,strategyVerified:true},{...fact,supported:null}])expect(renderToStaticMarkup(createElement(StrategyLifecycleNotice,{value}))).toContain('未知：');
 expect(renderToStaticMarkup(createElement(StrategyLifecycleNotice,{value:fact,unavailable:true}))).toContain('未知：');
 expect(renderToStaticMarkup(createElement(StrategyLifecycleNotice,{value:{...fact,supported:false}}))).toContain('未报告此策略的派发入口可用');
 expect(mocks.submits).toBe(0);
});

it('requires observed summary support for START while preserving STOP and original history',()=>{
 const old={supported:false,reason:'account_bound_runner_summary_upgrade_required',nativeSessionId:'original'};
 expect(accountSummaryStartAllowed(old)).toBe(false);
 expect(accountSummaryStartAllowed(undefined)).toBe(false);
 expect(accountSummaryStartAllowed({supported:true,nativeSessionId:'original'},true)).toBe(false);
 expect(accountSummaryStartAllowed({supported:true})).toBe(false);
 expect(accountSummaryStartAllowed({supported:true,nativeSessionId:'original'})).toBe(true);
 const html=renderToStaticMarkup(createElement(AccountObservationNotice,{value:old}));
 expect(html).toContain('自动资金观察不可用'); expect(html).toContain('不能热接管');
 expect(html).toContain('UNKNOWN');expect(html).toContain('停止策略、原订单分页和原回执入口保留');
 const stale=renderToStaticMarkup(createElement(AccountObservationNotice,{value:old,unavailable:true}));
 expect(stale).toContain('未知');expect(stale).not.toContain('原绑定运行器不支持有界账户观察：');
 expect(strategyResultText({requestId:'old',action:'RECOVER',status:'COMPLETED',receipt:{result:{accountObservation:old}},error:null})).toContain('不能热接管');
});

it('renders disabled paper START for legacy/unknown summary, without disabling STOP or sending',()=>{
 for(const supported of [false,null,true]) {
  mocks.observation={supported,nativeSessionId:'original'};
  const html=renderToStaticMarkup(createElement(AccountStrategy,{value:{...value,mode:'paper'} as ObservationView}));
  const buttons=[...html.matchAll(/<button\b[^>]*>[\s\S]*?<\/button>/g)].map(match=>match[0]);
  const start=buttons.find(button=>button.includes('启动模拟策略'));const stop=buttons.find(button=>button.includes('停止策略'));
  expect(start).toBeDefined();expect(stop).toBeDefined();
  expect(start!.includes('disabled=""')).toBe(supported!==true);
  expect(stop).not.toContain('disabled=""');expect(mocks.submits).toBe(0);
 }
 mocks.observation=undefined;
});

it('legacy full-history observation is explicit and disabled for unknown/current summary sessions',()=>{
 for(const capability of [undefined,{supported:true,nativeSessionId:'original'},{supported:false,nativeSessionId:'original'}]) {
  const html=renderToStaticMarkup(createElement(LegacyAccountObservation,{path:'/original/strategy',capability}));
  expect(html).toContain('全部订单与成交历史');expect(html).toContain('不是有界读取');expect(html).toContain('不用于 START 准入');
  expect(html.includes('disabled=""')).toBe(capability?.supported!==false);
 }
 expect(mocks.submits).toBe(0);
});
