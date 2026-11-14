import {createElement} from 'react';
import {renderToStaticMarkup} from 'react-dom/server';
import {afterEach,it,expect,vi} from 'vitest';
const state=vi.hoisted(()=>({slots:[] as unknown[],cursor:0,buttons:[] as any[],confirm:undefined as undefined|(()=>void)}));
// Controlled hook slots and explicit SSR renders; not browser scheduling or reload.
vi.mock('react',async()=>({...await vi.importActual<object>('react'),
 useState:(initial:unknown)=>{const i=state.cursor++;if(!(i in state.slots))state.slots[i]=initial;return [state.slots[i],(next:unknown)=>{state.slots[i]=next;}];},
 useRef:(initial:unknown)=>{const i=state.cursor++;if(!(i in state.slots))state.slots[i]={current:initial};return state.slots[i];},
}));
vi.mock('antd',()=>({Input:()=>null,Button:(p:any)=>{state.buttons.push(p);return null;}}));
vi.mock('./ui',()=>({useConfirm:()=> (_title:string,_description:string,run:()=>void)=>{state.confirm=run;},ErrorNotice:()=>null,PendingCommand:()=>null,Status:()=>null,Facts:()=>null}));
import {LegacyAccountObservation} from './account-strategy';
afterEach(()=>vi.unstubAllGlobals());
it('requires explicit confirmation for one original-session POST, suppresses double dispatch and never retries failure on render',async()=>{
 const requests:{url:string;body:unknown}[]=[];let finish:((r:Response)=>void)|undefined;
 vi.stubGlobal('fetch',(url:string,init:RequestInit)=>{expect(init.method).toBe('POST');requests.push({url,body:JSON.parse(init.body as string)});return new Promise<Response>(resolve=>{finish=resolve;});});
 const render=()=>{state.cursor=0;state.buttons=[];return renderToStaticMarkup(createElement(LegacyAccountObservation,{path:'/accounts/configurations/original/strategy',capability:{supported:false,nativeSessionId:'original-session'}}));};
 render();expect(requests).toHaveLength(0);state.buttons[0].onClick();expect(requests).toHaveLength(0);
 state.confirm!();state.confirm!();expect(requests).toEqual([{url:'/api/v3/accounts/configurations/original/strategy/legacy-snapshot',body:{mode:'LEGACY_FULL_SNAPSHOT',expectedNativeSessionId:'original-session',acknowledgeFullHistoryCost:true}}]);
 finish!(new Response(JSON.stringify({error:{code:'unavailable',message:'unavailable'}}),{status:503}));
 await vi.waitFor(()=>{expect(render()).toContain('没有自动重发');expect(state.buttons[0].loading).toBe(false);});
 render();render();expect(requests).toHaveLength(1);
});
