import {createElement} from 'react';
import {renderToStaticMarkup} from 'react-dom/server';
import {afterEach,beforeEach,it,expect,vi} from 'vitest';
const state=vi.hoisted(()=>({slots:[] as unknown[],cursor:0,buttons:[] as any[]}));
// Actual handlers/API with controlled hook slots; not browser scheduling.
vi.mock('react',async()=>({...await vi.importActual<object>('react'),
 useState:(initial:unknown)=>{const i=state.cursor++;if(!(i in state.slots))state.slots[i]=initial;return [state.slots[i],(next:unknown)=>{state.slots[i]=next;}];},
 useRef:(initial:unknown)=>{const i=state.cursor++;if(!(i in state.slots))state.slots[i]={current:initial};return state.slots[i];},
}));
vi.mock('antd',()=>({Button:(p:any)=>{state.buttons.push(p);return null;}}));
import {SpotBalanceFacts,SpotBalanceObservation} from './spot-balance-observation';
beforeEach(()=>{state.slots=[];state.cursor=0;state.buttons=[];});
afterEach(()=>vi.unstubAllGlobals());
it('shows exact official values and unknown/omitted currencies without financial computation',()=>{
 const show=(observation:unknown)=>renderToStaticMarkup(createElement(SpotBalanceFacts,{value:{observation} as any}));
 const row={currency:'USDC',total:'9007199254740993.000001 USDC',locked:'0.000001 USDC',free:'9007199254740993.000000 USDC'};
 const html=show({request_started_ns:'18446744073709551614',received_ns:'18446744073709551615',balances:[row],error:null});
 for(const value of [...Object.values(row),'18446744073709551614','18446744073709551615'])expect(html).toContain(value);
 expect(html).toContain('未核实当前新鲜度');expect(html).not.toContain('<pre');
 expect(show(null)).toContain('不等于余额为零');expect(show({balances:null})).toContain('余额未知');
 expect(show({balances:[]})).toContain('不能视为零余额');
 expect(show({balances:[row],error:'private diagnostics'})).not.toContain(row.total);
 expect(show({balances:[row],error:'private diagnostics'})).not.toContain('private diagnostics');
 const many=show({balances:Array.from({length:51},(_,i)=>({...row,currency:'TOKEN'+i}))});
 expect(many).toContain('TOKEN49');expect(many).not.toContain('TOKEN50');expect(many).toContain('前50项');
 expect(show({balances:[{currency:'USDC',total:100}]})).toContain('未知');
});
it('only explicit connected reads issue one GET, suppress duplicate clicks, and reject stale-source display',async()=>{
 const requests:{url:string;method:unknown}[]=[];let finish:((r:Response)=>void)|undefined;
 vi.stubGlobal('fetch',(url:string,init:RequestInit)=>{requests.push({url,method:init.method});return new Promise<Response>(resolve=>{finish=resolve;});});
 const render=(connected=true,sourceId='original',nativeSessionId:string|null='session-original')=>{state.cursor=0;state.buttons=[];return renderToStaticMarkup(createElement(SpotBalanceObservation,{path:'/accounts/configurations/original',sourceId,nativeSessionId,connected}));};
 render(false);state.buttons[0].onClick();expect(requests).toHaveLength(0);
 render();render();expect(requests).toHaveLength(0);state.buttons[0].onClick();state.buttons[0].onClick();
 expect(requests).toEqual([{url:'/api/v3/accounts/configurations/original/spot-balance-observation',method:'GET'}]);
 finish!(new Response(JSON.stringify({accountSourceId:'original',nativeSessionId:'session-original',observation:{balances:[{currency:'USDC',total:'EXACT-OLD-TOTAL'}],received_ns:'18446744073709551615'}}),{status:200}));
 await vi.waitFor(()=>expect(render()).toContain('EXACT-OLD-TOTAL'));
 expect(render(true,'replacement')).not.toContain('EXACT-OLD-TOTAL');expect(render(false)).not.toContain('EXACT-OLD-TOTAL');
 expect(render(true,'original','session-reconnected')).not.toContain('EXACT-OLD-TOTAL');expect(render(true,'original',null)).not.toContain('EXACT-OLD-TOTAL');
 expect(requests).toHaveLength(1);
 render();state.buttons[0].onClick();finish!(new Response(JSON.stringify({error:{code:'unavailable',message:'unavailable'}}),{status:503}));
 await vi.waitFor(()=>expect(render()).toContain('没有自动重发'));
 expect(render()).not.toContain('EXACT-OLD-TOTAL');expect(requests).toHaveLength(2);
});
