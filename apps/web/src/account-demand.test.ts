import {createElement} from 'react';
import {renderToStaticMarkup} from 'react-dom/server';
import {describe,it,expect,vi} from 'vitest';
const reads=vi.hoisted(()=>[] as Array<Record<string,unknown>>);
vi.mock('@tanstack/react-query',async importOriginal=>({...await importOriginal<typeof import('@tanstack/react-query')>(),useQuery:(options:Record<string,unknown>)=>{reads.push(options);return {};}}));
import {ResourceActivity,useResource} from './data-hooks';
function AccountReads(){
 for(const suffix of ['','/fence','/config','/orders?offset=0','/commands?offset=0'])useResource(`/accounts/configurations/paper/strategy${suffix}`,{poll:3000});
 useResource('/accounts/configurations/paper/strategy/commands/original',{enabled:false,poll:2000});
 return null;
}
describe('account reads follow panel demand without substituting stored state',()=>{
 it('pauses all hidden reads and refreshes every enabled read when shown again',()=>{
  for(const active of [false,true,false,true]){
   reads.length=0;renderToStaticMarkup(createElement(ResourceActivity.Provider,{value:active},createElement(AccountReads)));
   expect(reads).toHaveLength(6);
   expect(reads.slice(0,5).map(row=>row.enabled)).toEqual(Array(5).fill(active));
   expect(reads[5]!.enabled).toBe(false);
   for(const row of reads){expect(row.staleTime).toBe(0);expect(row.refetchIntervalInBackground).toBe(false);expect(row).not.toHaveProperty('initialData');expect(row).not.toHaveProperty('placeholderData');}
  }
 });
 it('does not change freshness options outside a demand panel',()=>{
  reads.length=0;renderToStaticMarkup(createElement(AccountReads));
  expect(reads[0]!.enabled).toBe(true);expect(reads[0]).not.toHaveProperty('staleTime');
 });
});
