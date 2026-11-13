import {createElement} from 'react';
import {renderToStaticMarkup} from 'react-dom/server';
import {describe,it,expect,vi} from 'vitest';
const calls=vi.hoisted(()=>[] as string[]);
vi.mock('./data-hooks',()=>({useResource:(path:string)=>{calls.push(path);return {refetch:async()=>{},data:path.includes('/orders?')?{available:false,items:[],binding:null}:path.endsWith('/commands?offset=0')?{items:[{requestId:'original-request',status:'UNCERTAIN',binding:{nativeSessionId:'old-session'},order:{clientOrderId:'disappeared-order'},receipt:{orderObservation:{status:'CANCELED',filledQuantity:'0.25'}}}],nextOffset:null}:undefined};}}));
import AccountOrders from './account-orders';
describe('retained cancellation history',()=>{
 it('keeps original UNKNOWN entry and read-only lookup when session and order rows disappear',()=>{
  const html=renderToStaticMarkup(createElement(AccountOrders,{accountId:'saved-paper'}));
  for(const text of ['original-request','old-session','disappeared-order','CANCELED','只读核对原回执','原取消请求 ID'])expect(html).toContain(text);
  expect(html).not.toContain('请求取消原单');expect(calls).toContain('/accounts/configurations/saved-paper/strategy/commands?offset=0');
 });
});
