import {createElement} from 'react';
import {renderToStaticMarkup} from 'react-dom/server';
import {act,create,type ReactTestRenderer} from 'react-test-renderer';
import {afterEach,expect,it,vi} from 'vitest';
import i18n from './i18n';
import NativeResultCharts,{NativeEvidenceCell} from './native-result-charts';
const state=vi.hoisted(()=>({paths:[] as string[],failed:false,crossPage:false,mixed:false}));
vi.mock('./research-chart',()=>({default:()=>createElement('div',{'data-chart':true},'page chart')}));
vi.mock('./ui',()=>({ErrorNotice:({error}:{error:unknown})=>error?createElement('p',null,'read failed'):null,Loading:()=>createElement('p',null,'loading')}));
vi.mock('antd',()=>({Alert:({title}:{title:string})=>createElement('p',null,title),Button:(p:any)=>createElement('button',p,p.children),Select:(p:any)=>createElement('select',p),Table:()=>null}));
vi.mock('./data-hooks',()=>({useResource:(path:string)=>{
 state.paths.push(path);const task=Number(new URL(`https://example.invalid${path}`).searchParams.get('taskId'));
 if(path.includes('/native-backtests')&&state.mixed)return {isPending:false,refetch:()=>{},data:{items:[{taskId:22,ownerTaskId:11,status:'completed',evidenceHref:''},{taskId:24,ownerTaskId:11,status:'failed',evidenceHref:''}],nextCursor:null}};
 if(path.includes('/native-backtests')&&state.crossPage)return {isPending:false,refetch:()=>{},data:{items:[{taskId:22,ownerTaskId:11,status:'completed',evidenceHref:''}],nextCursor:'next-original-page'}};
 if(path.includes('/native-backtests'))return {isPending:false,refetch:()=>{},data:{items:[{taskId:22,ownerTaskId:11,status:'completed',evidenceHref:''},{taskId:23,ownerTaskId:11,status:'completed',evidenceHref:''},{taskId:24,ownerTaskId:11,status:'failed',evidenceHref:''}],nextCursor:null}};
 const q=new URL(`https://example.invalid${path}`).searchParams,p=q.get('path'),offset=Number(q.get('offset'));
 if(p&&state.failed)return {isPending:false,error:new Error('failed'),refetch:()=>{},data:undefined};
 return {isPending:false,refetch:()=>{},data:{taskId:task,path:p,kind:p?'ROWS':'TABLES',offset,endExclusive:offset+1,totalRows:p?2:1,nextOffset:p&&offset===0?1:null,maxPayloadBytes:51200,scope:'ORIGINAL_TASK_ROWS',rows:p?[{ts_event_ns:'1776124800000000001',value:0.01}]:[{name:'equity_returns',path:'$.equity_performance.returns',rowCount:2,resultPath:'$'}]}};
}}));
let view:ReactTestRenderer|undefined;
afterEach(async()=>{if(view)await act(async()=>view!.unmount());view=undefined;state.paths=[];state.failed=false;state.crossPage=false;state.mixed=false;await i18n.changeLanguage('en');});
it('distinguishes an absent original field from explicit null',async()=>{
 await i18n.changeLanguage('en');
 expect(renderToStaticMarkup(createElement(NativeEvidenceCell,{value:null}))).toContain('null');
 expect(renderToStaticMarkup(createElement(NativeEvidenceCell,{value:undefined,present:false}))).toContain('Field absent');
});
it('uses explicit native child identities, actual nextOffset, and resets pages on task switch',async()=>{
 await act(async()=>{view=create(createElement(NativeResultCharts,{researchId:1,taskId:11}));});
 expect(state.paths.some(path=>path.includes('native-tables')), 'two parameter cases require explicit selection').toBe(false);
 const select=()=>view!.root.findAllByType('select')[0]!;
 await act(async()=>select().props.onChange(22));
 expect(state.paths.some(path=>path.includes('taskId=22&path=')&&path.includes('offset=0'))).toBe(true);
 const next=view!.root.findAllByType('button').find(button=>button.props.children==='Next page')!;
 await act(async()=>next.props.onClick());
 expect(state.paths.at(-1)).toContain('offset=1');
 await act(async()=>select().props.onChange(23));
 expect(state.paths.at(-1)).toContain('taskId=23&path=');expect(state.paths.at(-1)).toContain('offset=0');
 state.failed=true;
 await act(async()=>view!.update(createElement(NativeResultCharts,{researchId:1,taskId:11})));
 expect(JSON.stringify(view!.toJSON())).toContain('read failed');expect(JSON.stringify(view!.toJSON())).not.toContain('page chart');
 await act(async()=>select().props.onChange(24));
 expect(JSON.stringify(view!.toJSON())).toContain('did not complete successfully');
 expect(state.paths.some(path=>path.includes('taskId=11&'))).toBe(false);
});

it('does not choose the only case on a partial run-list page', async()=>{
 state.crossPage=true;
 await act(async()=>{view=create(createElement(NativeResultCharts,{researchId:1,taskId:11}));});
 expect(state.paths.some(path=>path.includes('native-tables'))).toBe(false);
 expect(view!.root.findByType('select').props.value).toBeUndefined();
});

it('does not select the surviving successful case from a failed batch', async()=>{
 state.mixed=true;
 await act(async()=>{view=create(createElement(NativeResultCharts,{researchId:1,taskId:11}));});
 expect(state.paths.some(path=>path.includes('native-tables'))).toBe(false);
 expect(view!.root.findByType('select').props.value).toBeUndefined();
});
