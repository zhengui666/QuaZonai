/** Transport metadata only. Business request identities and receipts remain owned by the account. */
export interface RemoteAccountBinding {
 deploymentId:string; sourceId:string; clientId:string; accountId:string;
 runnerIdentity:string; nativeSessionId:string|null;
}
export type RemoteAccountFrame =
 | {version:1;requestId:string;kind:'HELLO'}
 | {version:1;requestId:string;kind:'ACCOUNT_REQUEST';binding:RemoteAccountBinding;method:'GET'|'POST';path:string;body?:Record<string,unknown>};
export interface RemoteAccountReply {
 version:1;requestId:string;ok:boolean;binding?:RemoteAccountBinding;status?:number;value?:unknown;error?:string;
}
export const MAX_REMOTE_ACCOUNT_FRAME_BYTES=1_048_576;
export const MAX_REMOTE_ACCOUNT_REPLY_BYTES=8_388_608;
const keys=(value:Record<string,unknown>,allowed:string[])=>Object.keys(value).every(k=>allowed.includes(k));
const object=(value:unknown):value is Record<string,unknown>=>!!value&&typeof value==='object'&&!Array.isArray(value);
const text=(v:unknown):v is string=>typeof v==='string'&&v.length>0&&v.length<=512;
export function remoteAccountFrame(value:unknown):RemoteAccountFrame {
 if(!object(value)||value.version!==1||!text(value.requestId))throw Error('remote_account_frame_invalid');
 if(value.kind==='HELLO'&&keys(value,['version','requestId','kind']))return value as unknown as RemoteAccountFrame;
 if(value.kind!=='ACCOUNT_REQUEST'||!keys(value,['version','requestId','kind','binding','method','path','body'])||!object(value.binding))throw Error('remote_account_frame_invalid');
 const b=value.binding;
 if(!keys(b,['deploymentId','sourceId','clientId','accountId','runnerIdentity','nativeSessionId'])||!['deploymentId','sourceId','clientId','accountId'].every(k=>text(b[k]))||typeof b.runnerIdentity!=='string'||b.runnerIdentity.length<1||b.runnerIdentity.length>8192||!(b.nativeSessionId===null||text(b.nativeSessionId)))throw Error('remote_account_binding_invalid');
 if(!remoteAccountRouteAllowed(value.method,value.path,value.body))throw Error('remote_account_route_not_allowed');
 return value as unknown as RemoteAccountFrame;
}
export function remoteAccountRouteAllowed(method:unknown,path:unknown,body:unknown):boolean {
 if(typeof path!=='string'||path.length>2048)return false;
 if(method==='GET')return body===undefined&&(['/strategy/config','/strategy/fence','/strategy','/capabilities','/account'].includes(path)||/^\/strategy\/commands\/[A-Za-z0-9._~%\-]+$/.test(path)||/^\/strategy\/orders\/(0|[1-9][0-9]{0,14})$/.test(path));
 if(method!=='POST'||!object(body)||!text(body.requestId))return false;
 if(path==='/strategy/config')return true;
 return path==='/strategy/commands'&&['START','STOP','RECOVER','CANCEL'].includes(String(body.action));
}
export function sameRemoteAccountBinding(a:RemoteAccountBinding,b:RemoteAccountBinding):boolean {
 return a.deploymentId===b.deploymentId&&a.sourceId===b.sourceId&&a.clientId===b.clientId&&a.accountId===b.accountId&&a.runnerIdentity===b.runnerIdentity&&a.nativeSessionId===b.nativeSessionId;
}

export function remoteAccountReply(value:unknown,expectedRequestId:string):RemoteAccountReply {
 if(!object(value)||!keys(value,['version','requestId','ok','binding','status','value','error'])||value.version!==1||value.requestId!==expectedRequestId||typeof value.ok!=='boolean')throw Error('remote_account_reply_invalid');
 if(value.status!==undefined&&(!Number.isInteger(value.status)||Number(value.status)<100||Number(value.status)>599)||value.error!==undefined&&!text(value.error))throw Error('remote_account_reply_invalid');
 if(value.binding!==undefined)remoteAccountFrame({version:1,requestId:expectedRequestId,kind:'ACCOUNT_REQUEST',binding:value.binding,method:'GET',path:'/strategy/config'});
 if(value.ok&&!value.binding)throw Error('remote_account_reply_binding_missing');
 return value as unknown as RemoteAccountReply;
}
