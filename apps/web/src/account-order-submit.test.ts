import {createElement} from 'react';
import {renderToStaticMarkup} from 'react-dom/server';
import {afterEach,it,expect,vi} from 'vitest';
const state=vi.hoisted(()=>({buttons:[] as {disabled?:boolean;onClick?:()=>void;children:unknown}[],confirm:undefined as undefined|(()=>void),reads:[] as string[]}));
// Render the actual command hook in a non-browser harness; subscriptions are
// sampled on each explicit SSR remount, not an emulation of browser scheduling.
vi.mock('react',async()=>({...await vi.importActual<object>('react'),useSyncExternalStore:(_subscribe:unknown,get:()=>unknown)=>get()}));
vi.mock('antd',()=>({Button:(props:{disabled?:boolean;onClick?:()=>void;children:unknown})=>{state.buttons.push(props);return null;},Table:()=>null,Input:()=>null}));
vi.mock('./ui',()=>({useConfirm:()=> (_title:string,_text:string,accept:()=>void)=>{state.confirm=accept;},ErrorNotice:()=>null,Status:({value}:{value:string})=>value}));
vi.mock('./data-hooks',()=>({useResource:(path:string)=>{state.reads.push(path);return {refetch:async()=>{},data:{status:'UNCERTAIN',receipt:{orderObservation:{status:'CANCELED',filledQuantity:'0.25'}}}};}}));
import {CancelOrder} from './account-orders';
afterEach(()=>vi.unstubAllGlobals());
it('explicit confirm uses real command/API once and remount retains the original request for read only',async()=>{
 const storage=new Map<string,string>();vi.stubGlobal('sessionStorage',{getItem:(k:string)=>storage.get(k)??null,setItem:(k:string,v:string)=>storage.set(k,v),removeItem:(k:string)=>storage.delete(k)});
 const requests:{url:string;body:Record<string,unknown>}[]=[];
 vi.stubGlobal('fetch',async(url:string,init:RequestInit)=>{const body=JSON.parse(init.body as string);requests.push({url,body});return new Response(JSON.stringify({requestId:body.requestId,status:'UNCERTAIN'}),{status:202,headers:{'content-type':'application/json'}});});
 const binding={sourceId:'original-source',buildId:'original-build',environmentId:'Sandbox',nativeSessionId:'original-session'};
 const page={available:true,reason:null,mode:'paper',binding,capability:{supported:true},items:[],offset:0,nextOffset:null,total:null};
 const order={trader_id:'T-001',strategy_id:'ORIGINAL-001',account_id:'PAPER-001',instrument_id:'TEST.SIM',client_order_id:'only-order',venue_order_id:'original-venue',status:'ACCEPTED',quantity:'0.25',filled_quantity:'0',is_active_local:false,emulation_trigger:null,exec_algorithm_id:null};
 const render=()=>{state.buttons=[];return renderToStaticMarkup(createElement(CancelOrder,{accountId:'original/account',page,order}));};
 render();expect(requests).toHaveLength(0);expect(state.buttons[0]!.disabled).toBe(false);
 state.buttons[0]!.onClick!();expect(requests).toHaveLength(0);expect(state.confirm).toBeTypeOf('function');state.confirm!();
 await vi.waitFor(()=>expect(storage.has('quazonai.draft.strategy:configured:original/account:CANCEL:original-session:only-order')).toBe(true));
 expect(requests).toHaveLength(1);expect(requests[0]!.url).toBe('/api/v3/accounts/configurations/original%2Faccount/strategy/commands');
 expect(requests[0]!.body).toMatchObject({action:'CANCEL',strategyId:'ORIGINAL-001',binding,order:{traderId:'T-001',accountId:'PAPER-001',instrumentId:'TEST.SIM',clientOrderId:'only-order'}});
 const id=requests[0]!.body.requestId;expect(typeof id).toBe('string');
 const html=render();expect(state.buttons[0]!.disabled).toBe(true);expect(html).toContain(id as string);expect(html).toContain('UNCERTAIN');expect(html).toContain('CANCELED');
 expect(state.reads).toContain(`/accounts/configurations/original%2Faccount/strategy/commands/${id}`);expect(requests).toHaveLength(1);
});
