import {defineDoc,defineExtension,defineTask,defineTool,type JsonObject,type TaskId} from '@earendil-works/pi-durable';
import {compile,type Schema} from '@earendil-works/pi-schema';
import {readFile,mkdtemp,rm} from 'node:fs/promises';
import {join,normalize} from 'node:path';
import type {NativeTarget,NativeJson} from '../../../packages/contracts/src/nautilus.ts';
import {studyInput,object} from '../src/trials.ts';
import {validateNumbers} from '../src/input.ts';
import {nativeToolText,nativeToolOutputLimits} from '../src/native-result-view.ts';
import {executeNativeValue,nativeTargetForData,withNativeRemoteTracking,retainNativeRemoteUncertainty,nativeUncertainTaskId,reconcileNativeExecution,cancelUnsettledNativeExecutions} from '../src/native-execution.ts';

const schemas=JSON.parse(await readFile(new URL('../../../contracts/generated/domain-v1.openapi.json',import.meta.url),'utf8')).components.schemas;
function resolve(value:unknown):unknown {
  if(Array.isArray(value))return value.map(resolve);
  if(!value||typeof value!=='object')return value;
  const row=value as Record<string,unknown>;
  if(typeof row.$ref==='string')return resolve(schemas[row.$ref.split('/').at(-1)!]);
  return Object.fromEntries(Object.entries(row).map(([k,v])=>[k,resolve(v)]));
}
const schema=resolve(schemas.NativeMarketImportRequestV1) as Schema<JsonObject>;
const validate=compile(schema);
export function prepareNativeMarketImport(args:unknown):JsonObject {
  validateNumbers(args);
  if(!validate.check(args))throw new Error('native_market_import_request_schema_invalid');
  const request=args as JsonObject;
  if((request.received_time===true)!=(request.maximum_receive_delay_ns!==undefined&&request.maximum_receive_delay_ns!==null))throw new Error('RECEIVED_TIME_REQUIRES_MAXIMUM_DELAY');
  return request;
}
const Child=defineDoc<{taskId:TaskId<JsonObject>|null}>({kind:'qz.native-market-import-child',version:1,scope:'task',initial:()=>({taskId:null}),checkpointWhen:(_v,_o,info)=>info.deltasSinceBase>=127});
export const nativeMarketImportTool=defineTool({
  name:'native_market_import',outputLimits:nativeToolOutputLimits,parameters:schema,replay:'safe',executionMode:'sequential',
  description:'Import explicitly owner-specified existing local manifest and native instrument definitions into a NEW TradeTick catalog using the installed Nautilus native importer. No network acquisition, account-fill normalization, bars, trading, auto-registration or research qualification. Source-specific semantics and permission must already be declared in the manifest; raw account fills are not market trades. Use only authorized training paths/windows. Preserve incomplete outputs and original receipts; never retry an uncertain execution into another output directory to hide its state. Read the native report before separately registering any prepared dataset. A runner must explicitly declare preparation.marketImport.',
  prepareArguments:prepareNativeMarketImport,
  async execute(args,api,context){
    const input=await studyInput(api,context);
    if(input.mode==='LITERATURE_ONLY')throw new Error('literature_only_native_execution_forbidden');
    const target=nativeTargetForData(input.data);
    if(typeof target==='string'||!('kind' in target)||target.kind!=='RUST_PROCESS'||target.capabilities.preparation?.marketImport!==true)throw new Error('native_market_import_not_available');
    const id=await api.commit(async tx=>{const child=await tx.doc(Child,api.taskId);child.taskId??=await tx.createTask(NativeMarketImportTask,{target,request:args},{ownership:{kind:'task',taskId:api.taskId}});return child.taskId;},context);
    const task=await api.waitForTask(id,context);
    if(task.state.outcome.status!=='completed')throw new Error(task.state.outcome.status==='failed'?task.state.outcome.error.message:'native_market_import_aborted');
    return {output:[{type:'text',text:nativeToolText(task.state.outcome.result,id)}]};
  },
});
export const NativeMarketImportTask=defineTask<{target:NativeTarget;request:JsonObject},{phase:'compute'}|{phase:'reconcile';nativeTaskId:TaskId<NativeJson>},JsonObject>({
  name:'qz.native-market-import',version:1,initial:()=>({phase:'compute'}),
  phases:{async compute(task,runtime,context){
    const directory=await mkdtemp(join(process.env.PI_SCRATCH_DIR??process.env.QZ_SCRATCH_DIR??'/var/tmp','qz-market-import-'));
    try{
      const result=object(await withNativeRemoteTracking(task.id,runtime,context,()=>executeNativeValue(task.input.target,'native-market-import',task.input.request,[],directory,context)),'native_market_import_result');
      if(result.schema_version!==1||result.research_qualified!==false||typeof result.catalog_path!=='string'||normalize(result.catalog_path)!==join(String(task.input.request.output_path),'catalog')||typeof result.report_path!=='string'||object(result.report,'native_market_import_report').status!=='IMPORTED')throw new Error('native_market_import_result_invalid');
      await runtime.commit(()=>({status:'terminal',outcome:{status:'completed',result}}),context);
    }catch(error){
      await retainNativeRemoteUncertainty(error,task.id,runtime,context);if(runtime.signal.aborted)throw error;
      const nativeTaskId=nativeUncertainTaskId(error);
      if(nativeTaskId!==null){await runtime.commit(()=>({status:'sleeping',until:runtime.now()+1000,checkpoint:{phase:'reconcile',nativeTaskId}}),context);return;}
      await runtime.commit(()=>({status:'terminal',outcome:{status:'failed',error:{message:error instanceof Error?error.message:'native_market_import_failed'}}}),context);
    }finally{await rm(directory,{recursive:true,force:true});}
  },async reconcile(task,runtime,context){
    if(task.state.checkpoint.phase!=='reconcile')throw new Error('native_reconciliation_checkpoint_invalid');
    const settled=await reconcileNativeExecution(task.id,runtime,task.state.checkpoint.nativeTaskId,context);
    await runtime.commit(()=>settled?{status:'running',checkpoint:{phase:'compute'}}:{status:'sleeping',until:runtime.now()+1000,checkpoint:task.state.checkpoint},context);
  }},
  async abort(task,runtime,context){await cancelUnsettledNativeExecutions(task.id,runtime,context);await runtime.commit(()=>({status:'terminal',outcome:{status:'aborted'}}),context);},
});
export default defineExtension({name:'qz.native-market-import',tasks:[NativeMarketImportTask],tools:[nativeMarketImportTool]});
