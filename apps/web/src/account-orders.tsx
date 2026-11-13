import {useState} from 'react';
import {Button,Table,Input} from 'antd';
import {nativeStrategyControl,type NativeSessionBinding} from '../../../packages/contracts/src/native-strategy-control.ts';
import {useResource} from './data-hooks';
import {readDraft,saveDraft,useCommand} from './commands';
import {ErrorNotice,Status,useConfirm} from './ui';
type Order={trader_id?:string;strategy_id?:string;account_id?:string|null;instrument_id:string;client_order_id:string;venue_order_id:string|null;status:string;quantity:string;filled_quantity:string;is_active_local?:boolean;emulation_trigger?:string|null;exec_algorithm_id?:string|null};
type Page={available:boolean;reason:string|null;mode:string;binding:NativeSessionBinding|null;capability:{supported?:boolean;scope?:string;compatibilityVerified?:false};items:Order[];offset:number;nextOffset:number|null;total:number|null};
export function cancellationInput(page:Page,order:Order){
 if(!page.available||page.mode!=='paper'||page.capability.supported!==true||!page.binding||!['ACCEPTED','PARTIALLY_FILLED'].includes(order.status)||!order.venue_order_id||order.is_active_local!==false||order.emulation_trigger!==null||order.exec_algorithm_id!==null)return null;
 try{const {requestId:_,...input}=nativeStrategyControl({schemaVersion:1,requestId:'validate-only',action:'CANCEL',strategyId:order.strategy_id,binding:page.binding,order:{traderId:order.trader_id,accountId:order.account_id,instrumentId:order.instrument_id,clientOrderId:order.client_order_id}});return input;}catch{return null;}
}
type CancelPersistence={persisted?:boolean;error?:string};
export function cancellationPersistenceNotice(receipt:CancelPersistence|null|undefined){
 return receipt?.persisted===false?'本次取消回执或观察尚未确认持久保存。保留原请求和 UNKNOWN，不代表取消未执行，也不清除已保存的历史；只核对原回执，不重新发送。':null;
}
function CancellationPersistenceNotice({receipt}:{receipt:CancelPersistence|null|undefined}){
 const text=cancellationPersistenceNotice(receipt);return text?<p role="alert">{text}</p>:null;
}
export function CancelOrder({accountId,page,order}:{accountId:string;page:Page;order:Order}){
 const key=`strategy:configured:${accountId}:CANCEL:${page.binding!.nativeSessionId}:${order.client_order_id}` as const;
 const path=`/accounts/configurations/${encodeURIComponent(accountId)}/strategy/commands`;
 const [last,setLast]=useState(()=>readDraft<string>(key));const confirm=useConfirm();
 const command=useCommand<{requestId:string}>(key,r=>{saveDraft(key,r.requestId);setLast(r.requestId);},path);
 const requestId=command.values?.requestId as string|undefined ??last;
 const receipt=useResource<{status:string;error:string|null;receipt:{orderObservation?:{status:string;filledQuantity:string};error?:string;persisted?:boolean}|null}>(`${path}/${encodeURIComponent(requestId??'')}`,{enabled:!!requestId,poll:3000});
 const input=cancellationInput(page,order);
 return <div><Button disabled={!input||!!requestId||command.busy||!!command.pending} onClick={()=>confirm('取消这笔原模拟订单？',`原订单 ${order.client_order_id}，策略 ${order.strategy_id}。只向当前原会话发送本单取消请求；成交可能先于取消，官方终态不等于该请求成功或资金释放。`,()=>{if(input)void command.submit(input);})}>请求取消原单</Button>
 {requestId&&<><p>原请求：{requestId}。只查询，不自动重发。</p>{receipt.data&&<p>请求状态：<Status value={receipt.data.status}/>；官方订单：{receipt.data.receipt?.orderObservation?.status??'未取得'}；已成交：{receipt.data.receipt?.orderObservation?.filledQuantity??'未知'}。保留原 UNKNOWN，不推断资金释放。</p>}<Button onClick={()=>void receipt.refetch()}>核对原回执</Button></>}
 <CancellationPersistenceNotice receipt={receipt.data?.receipt}/>{!input&&!requestId&&<small>当前原身份、会话或普通订单条件不足，不能取消。</small>}<ErrorNotice error={command.error??receipt.error}/></div>;
}
type CancelReceipt={requestId:string;status:string;binding?:{nativeSessionId:string};order?:{clientOrderId:string};receipt?:{persisted?:boolean;error?:string;orderObservation?:{status:string;filledQuantity:string}}|null};
export function CancellationHistory({accountId}:{accountId:string}){
 const [offset,setOffset]=useState(0);const [requestId,setRequestId]=useState('');const [query,setQuery]=useState('');
 const path=`/accounts/configurations/${encodeURIComponent(accountId)}/strategy/commands`;
 const history=useResource<{items:CancelReceipt[];nextOffset:number|null}>(`${path}?offset=${offset}`,{poll:3000});
 const detail=useResource<CancelReceipt>(`${path}/${encodeURIComponent(query)}`,{enabled:!!query});
 return <section><h4>原取消请求与未决回执</h4><p>独立保留原请求，不依赖当前订单行或存活会话。官方订单终态不代表原请求已确认，更不代表资金释放。</p>
 <ErrorNotice error={history.error}/><Table<CancelReceipt> rowKey="requestId" pagination={false} dataSource={history.data?.items??[]} scroll={{x:600}} columns={[
 {title:'原请求',dataIndex:'requestId'},{title:'原会话',render:(_,r)=>r.binding?.nativeSessionId??'未提供'},{title:'原订单',render:(_,r)=>r.order?.clientOrderId??'未提供'},
 {title:'请求状态',dataIndex:'status',render:v=><Status value={v}/>},{title:'官方观察',render:(_,r)=><>{r.receipt?.orderObservation?.status??'未取得'}<CancellationPersistenceNotice receipt={r.receipt}/></>},
 {title:'操作',render:(_,r)=><Button onClick={()=>setQuery(r.requestId)}>只读核对原回执</Button>}
 ]}/><Button disabled={offset===0} onClick={()=>setOffset(Math.max(0,offset-50))}>较早记录</Button><Button disabled={!history.data||history.data.nextOffset===null} onClick={()=>setOffset(history.data!.nextOffset!)}>更多记录</Button>
 <Input aria-label="原取消请求 ID" value={requestId} onChange={e=>setRequestId(e.target.value)} placeholder="也可输入保留的原请求 ID"/><Button disabled={!requestId} onClick={()=>setQuery(requestId)}>只读查询</Button>
 {detail.data&&<p>原请求 {detail.data.requestId}：<Status value={detail.data.status}/>；官方订单 {detail.data.receipt?.orderObservation?.status??'未取得'}；已成交 {detail.data.receipt?.orderObservation?.filledQuantity??'未知'}。</p>}
 <CancellationPersistenceNotice receipt={detail.data?.receipt}/>{!!query&&<Button onClick={()=>void detail.refetch()}>刷新原回执</Button>}<ErrorNotice error={detail.error}/></section>;
}
export default function AccountOrders({accountId}:{accountId:string}){
 const [offset,setOffset]=useState(0);const page=useResource<Page>(`/accounts/configurations/${encodeURIComponent(accountId)}/strategy/orders?offset=${offset}`,{poll:3000});
 return <section className="result-section"><h3>原生订单与显式模拟取消</h3><p>这里只读取原节点订单，不连接或启动账户。每页最多50条；按原生实时缓存迭代，不是固定历史快照，变化期间翻页可能重复或遗漏，请返回首屏复核。仅已支持的普通模拟订单可请求取消，不支持实盘、模拟触发单或算法单。</p><ErrorNotice error={page.error} retry={()=>void page.refetch()}/>
 {page.data&&!page.data.available&&<p>原会话不可用，不能据此认定没有订单。请保留旧回执，不自动重连。</p>}
 {page.data?.available&&<><p>取消能力：{page.data.capability.supported?'当前节点报告支持受限模拟取消':'当前节点未报告支持'}。可信代码声明不是认证，最终身份与路由仍由原节点核对。</p><Table<Order> rowKey="client_order_id" pagination={false} dataSource={page.data.items} scroll={{x:750}} columns={[
 {title:'原订单',dataIndex:'client_order_id'},{title:'原策略',dataIndex:'strategy_id',render:v=>v??'原节点未提供'},{title:'标的',dataIndex:'instrument_id'},{title:'官方状态',dataIndex:'status'},{title:'数量',dataIndex:'quantity'},{title:'已成交',dataIndex:'filled_quantity'},
 {title:'操作',render:(_,order)=>page.data!.binding?<CancelOrder key={`${page.data!.binding.nativeSessionId}:${order.client_order_id}`} accountId={accountId} page={page.data!} order={order}/>:null},
 ]}/><Button disabled={offset===0} onClick={()=>setOffset(Math.max(0,offset-50))}>上一页</Button><Button disabled={page.data.nextOffset===null} onClick={()=>setOffset(page.data!.nextOffset!)}>下一页</Button></>}
 <CancellationHistory accountId={accountId}/>
 </section>;
}
