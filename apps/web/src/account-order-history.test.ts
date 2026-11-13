import {createElement} from 'react';
import {renderToStaticMarkup} from 'react-dom/server';
import {describe,it,expect,vi} from 'vitest';
const calls=vi.hoisted(()=>[] as string[]);
const fixture=vi.hoisted(()=>({persisted:true}));
vi.mock('./data-hooks',()=>({useResource:(path:string)=>{calls.push(path);return {refetch:async()=>{},data:path.includes('/orders?')?{available:false,items:[],binding:null}:path.endsWith('/commands?offset=0')?{items:[{requestId:'original-request',status:'UNCERTAIN',binding:{nativeSessionId:'old-session'},order:{clientOrderId:'disappeared-order'},receipt:{persisted:fixture.persisted,orderObservation:{status:'CANCELED',filledQuantity:'0.25'}}}],nextOffset:null}:undefined};}}));
import AccountOrders,{cancellationPersistenceNotice} from './account-orders';
describe('retained cancellation history',()=>{
 it('distinguishes explicit unconfirmed persistence without claiming old records were lost',()=>{
  for(const receipt of [undefined,null,{}, {persisted:true}])expect(cancellationPersistenceNotice(receipt)).toBeNull();
  fixture.persisted=false;
  try{const html=renderToStaticMarkup(createElement(AccountOrders,{accountId:'saved-paper'}));
   expect(html).toContain('本次取消回执或观察尚未确认持久保存');expect(html).toContain('不代表取消未执行');
   expect(html).toContain('不清除已保存的历史');expect(html).toContain('CANCELED');expect(html).toContain('original-request');
   expect(html).toContain('role=\"alert\"');expect(html).not.toContain('请求取消原单');
  }finally{fixture.persisted=true;}
 });
 it('keeps original UNKNOWN entry and read-only lookup when session and order rows disappear',()=>{
  const html=renderToStaticMarkup(createElement(AccountOrders,{accountId:'saved-paper'}));
  for(const text of ['original-request','old-session','disappeared-order','CANCELED','只读核对原回执','原取消请求 ID'])expect(html).toContain(text);
  expect(html).not.toContain('请求取消原单');expect(calls).toContain('/accounts/configurations/saved-paper/strategy/commands?offset=0');
 });
});
