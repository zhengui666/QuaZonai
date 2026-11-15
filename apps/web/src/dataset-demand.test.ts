import {createElement} from 'react';
import {act,create,type ReactTestRenderer} from 'react-test-renderer';
import {QueryClient,QueryClientProvider} from '@tanstack/react-query';
import {afterEach,expect,it,vi} from 'vitest';
const network=vi.hoisted(()=>({calls:[] as string[],pending:new Map<string,Array<{resolve:(value:unknown)=>void;reject:(error:Error)=>void}>>()}));
vi.mock('./api',()=>({api:(path:string)=>{network.calls.push(path);if(path==='/datasets?view=summary')return Promise.resolve(['A','B'].map(id=>({id,name:id,ready:true,nativeModality:'TRADE_TICK'})));return new Promise((resolve,reject)=>{const rows=network.pending.get(path)??[];rows.push({resolve,reject});network.pending.set(path,rows);});}}));
vi.mock('antd',async()=>{const {createElement:h}=await import('react');return {
 Button:({children,onClick}:any)=>h('button',{onClick},children),
 Table:({dataSource,columns}:any)=>h('div',null,...dataSource.map((row:any)=>h('div',{key:row.id},columns[0].render(undefined,row)))),
 Drawer:({children,open,onClose}:any)=>open?h('section',{'aria-label':'drawer'},h('button',{onClick:onClose},'close'),children):null,
};});
vi.mock('./ui',async()=>{const {createElement:h}=await import('react');return {Copy:()=>null,Empty:()=>null,Facts:()=>null,Status:()=>null,Timestamp:()=>null,Loading:()=>h('span',null,'loading'),ErrorNotice:({error,retry}:any)=>error?h('button',{onClick:retry},'retry'):null};});
vi.mock('./native-input',async()=>{const {createElement:h}=await import('react');return {NativeInputFacts:({coverage}:any)=>h('span',null,coverage?.marker??'no-coverage')};});
import Data from './data';
let renderer:ReactTestRenderer|undefined,client:QueryClient|undefined;
async function settle(){await act(async()=>{await new Promise(resolve=>setTimeout(resolve,10));});}
async function click(name:string){await act(async()=>{renderer!.root.findAllByType('button').find(node=>node.children.some(child=>child===name))!.props.onClick();});await settle();}
async function mount(){vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT',true);network.calls.length=0;network.pending.clear();client=new QueryClient({defaultOptions:{queries:{retry:false,gcTime:0}}});await act(async()=>{renderer=create(createElement(QueryClientProvider,{client},createElement(Data)));});await settle();}
function text(){return JSON.stringify(renderer!.toJSON());}
afterEach(async()=>{if(renderer)await act(async()=>renderer!.unmount());client?.clear();renderer=undefined;vi.unstubAllGlobals();});
it('loads no detail before selection, isolates fast A/B changes, and ignores a late closed response',async()=>{
 await mount();expect(network.calls).toEqual(['/datasets?view=summary']);
 await click('A');expect(network.calls).toContain('/datasets/A');expect(text()).toContain('loading');
 await click('B');expect(network.calls).toContain('/datasets/B');
 await act(async()=>network.pending.get('/datasets/A')![0]!.resolve({id:'A',name:'A',inputCoverage:{marker:'coverage-A'}}));await settle();
 expect(text()).not.toContain('coverage-A');
 await act(async()=>network.pending.get('/datasets/B')![0]!.resolve({id:'B',name:'B',inputCoverage:{marker:'coverage-B'}}));await settle();expect(text()).toContain('coverage-B');
 await click('close');expect(text()).not.toContain('coverage-B');
 await click('A');await click('close');
 for(const request of network.pending.get('/datasets/A')!)request.resolve({id:'A',inputCoverage:{marker:'late-A'}});
 await settle();expect(text()).not.toContain('late-A');expect(renderer!.root.findAllByProps({'aria-label':'drawer'})).toHaveLength(0);
});
it('shows a failed detail with explicit retry and does not silently use another selection',async()=>{
 await mount();await click('A');await act(async()=>network.pending.get('/datasets/A')![0]!.reject(new Error('detail unavailable')));await settle();
 expect(text()).toContain('retry');expect(text()).not.toContain('no-coverage');
 await click('retry');expect(network.pending.get('/datasets/A')).toHaveLength(2);
 await act(async()=>network.pending.get('/datasets/A')![1]!.resolve({id:'A',inputCoverage:{marker:'recovered-A'}}));await settle();expect(text()).toContain('recovered-A');
});
