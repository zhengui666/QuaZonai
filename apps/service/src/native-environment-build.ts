import {mkdir} from 'node:fs/promises';
import {join} from 'node:path';
import {BACKGROUND_CONTEXT as context} from '@earendil-works/chord/context';
import {defineDoc,defineExtension,defineTask,type JsonObject,type TaskId} from '@earendil-works/pi-durable';
import {buildRustLauncher} from '../../nautilus/src/rust-launcher-build.ts';
import type {ResearchHost} from '../../research/src/host.ts';
import type {NativeRustEnvironment,ProcessNativeTarget} from '../../../packages/contracts/src/nautilus.ts';
import {validateNativeTarget} from '../../research/src/native-execution.ts';
import {canonical} from '../../research/src/documents.ts';
import {object,Problem,json} from './facts.ts';
import type {ServiceConfig} from './config.ts';
import type {instanceSettings} from './instance-settings.ts';

const BuildRequests=defineDoc<{requests:Record<string,{taskId:number,input:JsonObject}>}>({kind:'qz.native-environment-builds',version:1,scope:'session',initial:()=>({requests:{}}),checkpointWhen:()=>true});
type Input={requestId:string;revision:number;environment:NativeRustEnvironment};
/** Original official Pi task owns construction and uncertainty. No parallel job ledger. */
export function nativeEnvironmentBuild(config:ServiceConfig,build=buildRustLauncher){
 const Build=defineTask<Input,{phase:'compile'|'unknown'},JsonObject>({name:'qz.native-environment-build',version:1,initial:()=>({phase:'compile'}),phases:{
  async compile(task,runtime,ctx){
   // A lost build may have produced files; never automatically compile it twice.
   await runtime.commit(()=>({status:'running',checkpoint:{phase:'unknown'}}),ctx);
   const root=join(config.dataDir,'native-builds');await mkdir(root,{recursive:true});
   const selected=task.input.environment;
   try{
    const result=await build({source:selected,factories:{backtestFactory:selected.backtestFactory,accountFactory:selected.accountFactory},glueSourceRoot:process.env.QZ_NATIVE_GLUE_SOURCE??'/opt/quazonai/native-glue',directory:join(root,String(task.id)),signal:runtime.signal});
    const target=validateNativeTarget({kind:'RUST_PROCESS',id:selected.id,binary:result.executable,capabilities:result.capabilities}) as ProcessNativeTarget;
    await runtime.commit(()=>({status:'terminal',outcome:{status:'completed',result:json({qualification:result.qualification,target,manifestPath:result.manifestPath,connected:false,strategyStarted:false})}}),ctx);
   }catch(error){
    if(runtime.signal.aborted)throw error;
    // Diagnostics stay in the isolated build files; never expose user compiler stderr here.
    await runtime.commit(()=>({status:'terminal',outcome:{status:'failed',error:{message:'native_build_not_verified_inspect_original_build_directory'}}}),ctx);
   }
  },
  async unknown(_task,runtime,ctx){await runtime.commit(()=>({status:'terminal',outcome:{status:'failed',error:{message:'native_build_original_outcome_unknown_no_automatic_retry'}}}),ctx);},
 },async abort(_task,runtime,ctx){await runtime.commit(()=>({status:'terminal',outcome:{status:'aborted'}}),ctx);}});
 const extension=defineExtension({name:'qz.native-environment-build',tasks:[Build]});
 function workbench(host:ResearchHost,settings:Awaited<ReturnType<typeof instanceSettings>>){
  async function read(requestId:string){
   const original=(await host.harness.snapshot(BuildRequests,context))?.requests[requestId];if(!original)throw new Problem('native_build_request_not_found',404);
   const task=await host.harness.getTask(original.taskId as TaskId<JsonObject>,context);if(!task)throw new Problem('native_build_task_missing',409);
   return {requestId,taskId:task.id,input:original.input,state:task.state,connected:false,strategyStarted:false};
  }
  async function start(value:unknown){
   const body=object(value);if(Object.keys(body).some(k=>!['requestId','expectedRevision'].includes(k))||typeof body.requestId!=='string'||!body.requestId.trim()||!Number.isSafeInteger(body.expectedRevision))throw new Problem('native_build_request_invalid',422);
   const current=settings.view();const saved=(await host.harness.snapshot(BuildRequests,context))?.requests[body.requestId];
   if(saved){if(saved.input.revision!==body.expectedRevision)throw new Problem('request_id_conflict',409);return read(body.requestId);}
   if(current.revision!==body.expectedRevision)throw new Problem('instance_settings_revision_conflict',409);
   if(!current.environment)throw new Problem('rust_environment_not_configured',409);
   const input:Input={requestId:body.requestId,revision:current.revision,environment:current.environment};
   const root=await host.harness.root(context);
   await root.commit(async tx=>{const requests=await tx.doc(BuildRequests);const prior=requests.requests[input.requestId];if(prior){if(canonical(prior.input)!==canonical(json(input)))throw new Problem('request_id_conflict',409);return;}
    const taskId=await tx.createTask(Build,input,{ownership:{kind:'conversation'},background:true});requests.requests[input.requestId]={taskId,input:json(input)};
   },context);return read(input.requestId);
  }
  async function select(value:unknown){
   const body=object(value);if(typeof body.requestId!=='string'||!Number.isSafeInteger(body.expectedRevision)||Object.keys(body).some(k=>!['requestId','expectedRevision'].includes(k)))throw new Problem('native_build_selection_invalid',422);
   const result=await read(body.requestId);const current=settings.view();
   if(body.expectedRevision!==result.input.revision||canonical(result.input.environment as never)!==canonical(current.environment as never))throw new Problem('native_build_environment_changed',409);
   if(result.state.status!=='terminal'||result.state.outcome.status!=='completed')throw new Problem('native_build_not_completed',409);
   const target=validateNativeTarget(result.state.outcome.result.target) as ProcessNativeTarget;
   return settings.selectRunner(Number(body.expectedRevision),target);
  }
  return {start,read,select};
 }
 return {extension,workbench};
}
