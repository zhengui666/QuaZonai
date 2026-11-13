import {createElement} from 'react';
import {renderToStaticMarkup} from 'react-dom/server';
import {afterEach,it,expect,vi} from 'vitest';
const state=vi.hoisted(()=>({slots:[] as unknown[],cursor:0,buttons:[] as any[],inputs:[] as any[]}));
// Explicit non-browser renders retain hook slots. This exercises production
// handlers/useCommand/API, not React DOM scheduling, focus or storage reload.
vi.mock('react',async()=>({...await vi.importActual<object>('react'),
 useState:(initial:unknown)=>{const i=state.cursor++;if(!(i in state.slots))state.slots[i]=typeof initial==='function'?(initial as ()=>unknown)():initial;return [state.slots[i],(next:unknown)=>{state.slots[i]=typeof next==='function'?(next as (v:unknown)=>unknown)(state.slots[i]):next;}];},
 useRef:(initial:unknown)=>{const i=state.cursor++;if(!(i in state.slots))state.slots[i]={current:initial};return state.slots[i];},
 useSyncExternalStore:(_subscribe:unknown,get:()=>unknown)=>get(),
}));
vi.mock('antd',()=>{const Input=Object.assign((p:any)=>{state.inputs.push(p);return null;},{TextArea:(p:any)=>{state.inputs.push(p);return null;}});return {Input,Button:(p:any)=>{state.buttons.push(p);return null;},Checkbox:()=>null,Select:()=>null};});
vi.mock('./account-strategy',()=>({default:()=>null}));
vi.mock('./ui',()=>({ErrorNotice:()=>null,PendingCommand:()=>null}));
vi.mock('./data-hooks',()=>({useResource:()=>({data:[],refetch:async()=>{}})}));
import AccountConfiguration from './account-configuration';
afterEach(()=>vi.unstubAllGlobals());
it('parent form submits exact draft through original API, keeps pending bytes and never automatically repeats',async()=>{
 const storage=new Map<string,string>();vi.stubGlobal('sessionStorage',{getItem:(k:string)=>storage.get(k)??null,setItem:(k:string,v:string)=>storage.set(k,v),removeItem:(k:string)=>storage.delete(k)});
 const requests:{url:string;raw:string}[]=[];let finish:((response:Response)=>void)|undefined;
 vi.stubGlobal('fetch',(url:string,init:RequestInit)=>{expect(init.method).toBe('POST');requests.push({url,raw:init.body as string});return new Promise<Response>(resolve=>{finish=resolve;});});
 const render=()=>{state.cursor=0;state.buttons=[];state.inputs=[];renderToStaticMarkup(createElement(AccountConfiguration,{enabled:true}));};
 const input=(label:string)=>state.inputs.find(p=>p['aria-label']===label)!;
 const save=()=>state.buttons.find(p=>String(p.children).includes('账户保存请求')||p.children==='仅保存新账户配置')!;
 render();expect(requests).toHaveLength(0);
 const original={id:'original',clientId:'client',accountId:'PAPER-001',mode:'paper',instruments:['OLD.SIM'],startingBalance:'9007199254740993.00001',paperFeeRate:'0.000000000000000001',nativeStrategy:{instrument_id:'TEST.SIM',strategy:{kind:'REGISTERED',name:'owner',config:{opaque:['18446744073709551615']}}}};
 input('新账户配置 JSON').onChange({target:{value:JSON.stringify(original)}});render();
 for(const value of ['TEST.SIM','TEST.SIM\n','TEST.SIM\nOTHER.SIM']){input('订阅标的').onChange({target:{value}});render();expect(input('订阅标的').value).toBe(value);}
 input('本地账户名称').onChange({target:{value:'saved-name'}});render();expect(input('订阅标的').value).toBe('TEST.SIM\nOTHER.SIM');
 save().onClick();save().onClick();expect(requests).toHaveLength(1);
 expect(requests[0]!.url).toBe('/api/v3/accounts/configurations');
 const payload=JSON.parse(requests[0]!.raw);expect(payload.configuration).toEqual({...original,id:'saved-name',instruments:['TEST.SIM','OTHER.SIM']});expect(typeof payload.requestId).toBe('string');
 finish!(new Response(JSON.stringify({error:{code:'unavailable',message:'controlled unavailable'}}),{status:503}));
 await vi.waitFor(()=>{render();expect(save().loading).toBe(false);});
 expect(save().children).toBe('核实原账户保存请求');expect(input('订阅标的').disabled).toBe(true);expect(requests).toHaveLength(1);
 render();expect(requests).toHaveLength(1);expect(storage.get('quazonai.command.account-configuration.v3')).toBe(requests[0]!.raw);
 // Explicit verification uses the same idempotent POST, never a new intent.
 save().onClick();expect(requests).toHaveLength(2);expect(requests[1]).toEqual(requests[0]);
 finish!(new Response(JSON.stringify({id:'saved-name'}),{status:200}));
 await vi.waitFor(()=>expect(storage.has('quazonai.command.account-configuration.v3')).toBe(false));
 render();expect(requests).toHaveLength(2);
});
