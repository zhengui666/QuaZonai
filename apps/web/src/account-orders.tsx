import { useTranslation } from 'react-i18next';
import i18n from './i18n';
const t = (key: string, values?: Record<string, unknown>): string => i18n.t(key, { ns: 'settings', ...values });
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
 return receipt?.persisted===false?t("Persistence of this cancellation receipt or observation is unconfirmed. Keep the original request and UNKNOWN state. This does not mean cancellation was not executed, and saved history remains. Check the original receipt only; do not resend."):null;
}
function CancellationPersistenceNotice({receipt}:{receipt:CancelPersistence|null|undefined}){
 useTranslation('settings');

 const text=cancellationPersistenceNotice(receipt);return text?<p role="alert">{text}</p>:null;
}
export function CancelOrder({accountId,page,order}:{accountId:string;page:Page;order:Order}){
 const { t } = useTranslation('settings');

 const key=`strategy:configured:${accountId}:CANCEL:${page.binding!.nativeSessionId}:${order.client_order_id}` as const;
 const path=`/accounts/configurations/${encodeURIComponent(accountId)}/strategy/commands`;
 const [last,setLast]=useState(()=>readDraft<string>(key));const confirm=useConfirm();
 const command=useCommand<{requestId:string}>(key,r=>{saveDraft(key,r.requestId);setLast(r.requestId);},path);
 const requestId=command.values?.requestId as string|undefined ??last;
 const receipt=useResource<{status:string;error:string|null;receipt:{orderObservation?:{status:string;filledQuantity:string};error?:string;persisted?:boolean}|null}>(`${path}/${encodeURIComponent(requestId??'')}`,{enabled:!!requestId,poll:3000});
 const input=cancellationInput(page,order);
 return <div><Button disabled={!input||!!requestId||command.busy||!!command.pending} onClick={()=>confirm(t("Cancel this original paper order?"),t("Original order {{v0}}, strategy {{v1}}. Send only this cancellation request to the current original session. A fill may happen first. An official terminal state does not prove this request succeeded or funds were released.", { v0: order.client_order_id, v1: order.strategy_id }),()=>{if(input)void command.submit(input);})}>{t("Request cancellation of original order")}</Button>
 {requestId&&<><p>{t("Original request:")}{requestId}{t("Sentence end")}</p>{receipt.data&&<p>{t("Request status:")}<Status value={receipt.data.status}/>{t("; official order:")}{receipt.data.receipt?.orderObservation?.status??t("Not retrieved")}{t("; filled:")}{receipt.data.receipt?.orderObservation?.filledQuantity??t("Unknown")}{t("Sentence end")}</p>}<Button onClick={()=>void receipt.refetch()}>{t("Check original receipt")}</Button></>}
 <CancellationPersistenceNotice receipt={receipt.data?.receipt}/>{!input&&!requestId&&<small>{t("The original identity, session, or standard-order conditions do not allow cancellation.")}</small>}<ErrorNotice error={command.error??receipt.error}/></div>;
}
type CancelReceipt={requestId:string;status:string;binding?:{nativeSessionId:string};order?:{clientOrderId:string};receipt?:{persisted?:boolean;error?:string;orderObservation?:{status:string;filledQuantity:string}}|null};
export function CancellationHistory({accountId}:{accountId:string}){
 const { t } = useTranslation('settings');

 const [offset,setOffset]=useState(0);const [requestId,setRequestId]=useState('');const [query,setQuery]=useState('');
 const path=`/accounts/configurations/${encodeURIComponent(accountId)}/strategy/commands`;
 const history=useResource<{items:CancelReceipt[];nextOffset:number|null}>(`${path}?offset=${offset}`,{poll:3000});
 const detail=useResource<CancelReceipt>(`${path}/${encodeURIComponent(query)}`,{enabled:!!query});
 return <section><h4>{t("Original cancellation requests and pending receipts")}</h4>
 <ErrorNotice error={history.error}/><Table<CancelReceipt> rowKey="requestId" pagination={false} dataSource={history.data?.items??[]} scroll={{x:600}} columns={[
 {title:t("Original request"),dataIndex:'requestId'},{title:t("Original session"),render:(_,r)=>r.binding?.nativeSessionId??t("Not provided")},{title:t("Original order"),render:(_,r)=>r.order?.clientOrderId??t("Not provided")},
 {title:t("Request status"),dataIndex:'status',render:v=><Status value={v}/>},{title:t("Official observation"),render:(_,r)=><>{r.receipt?.orderObservation?.status??t("Not retrieved")}<CancellationPersistenceNotice receipt={r.receipt}/></>},
 {title:t("Actions"),render:(_,r)=><Button onClick={()=>setQuery(r.requestId)}>{t("Read original receipt")}</Button>}
 ]}/><Button disabled={offset===0} onClick={()=>setOffset(Math.max(0,offset-50))}>{t("Earlier records")}</Button><Button disabled={!history.data||history.data.nextOffset===null} onClick={()=>setOffset(history.data!.nextOffset!)}>{t("More records")}</Button>
 <Input aria-label={t("Original cancellation request ID")} value={requestId} onChange={e=>setRequestId(e.target.value)} placeholder={t("Or enter a retained original request ID")}/><Button disabled={!requestId} onClick={()=>setQuery(requestId)}>{t("Read-only lookup")}</Button>
 {detail.data&&<p>{t("Original request")}{detail.data.requestId}{t("Label separator")}<Status value={detail.data.status}/>{t("; official order")}{detail.data.receipt?.orderObservation?.status??t("Not retrieved")}{t("; filled")}{detail.data.receipt?.orderObservation?.filledQuantity??t("Unknown")}{t("Sentence end")}</p>}
 <CancellationPersistenceNotice receipt={detail.data?.receipt}/>{!!query&&<Button onClick={()=>void detail.refetch()}>{t("Refresh original receipt")}</Button>}<ErrorNotice error={detail.error}/></section>;
}
export default function AccountOrders({accountId,readOnly=false}:{accountId:string;readOnly?:boolean}){
 const { t } = useTranslation('settings');

 const [offset,setOffset]=useState(0);const page=useResource<Page>(`/accounts/configurations/${encodeURIComponent(accountId)}/strategy/orders?offset=${offset}`,{poll:3000});
 return <section className="result-section"><h3>{readOnly?t("Native orders (read-only)"):t("Native orders and explicit paper cancellation")}</h3><p>{readOnly?t("Remote cancellation is not available yet."):t("Only standard paper orders can be cancelled.")}{t("Live pagination may repeat or omit records.")}</p><ErrorNotice error={page.error} retry={()=>void page.refetch()}/>
 {page.data&&!page.data.available&&<p>{t("The original session is unavailable; order status is unknown.")}</p>}
 {page.data?.available&&<><p>{t("Cancellation support:")}{readOnly?t("This connection does not support cancellation"):page.data.capability.supported?t("This node reports limited paper cancellation support"):t("This node has not reported support")}{t("Sentence end")}</p><Table<Order> rowKey="client_order_id" pagination={false} dataSource={page.data.items} scroll={{x:750}} columns={[
 {title:t("Original order"),dataIndex:'client_order_id'},{title:t("Original strategy"),dataIndex:'strategy_id',render:v=>v??t("Not provided by the original node")},{title:t("Instrument"),dataIndex:'instrument_id'},{title:t("Official status"),dataIndex:'status'},{title:t("Quantity"),dataIndex:'quantity'},{title:t("Filled"),dataIndex:'filled_quantity'},
 {title:t("Actions"),render:(_,order)=>!readOnly&&page.data!.binding?<CancelOrder key={`${page.data!.binding.nativeSessionId}:${order.client_order_id}`} accountId={accountId} page={page.data!} order={order}/>:null},
 ]}/><Button disabled={offset===0} onClick={()=>setOffset(Math.max(0,offset-50))}>{t("Previous page")}</Button><Button disabled={page.data.nextOffset===null} onClick={()=>setOffset(page.data!.nextOffset!)}>{t("Next page")}</Button></>}
 {!readOnly&&<CancellationHistory accountId={accountId}/>}
 </section>;
}
