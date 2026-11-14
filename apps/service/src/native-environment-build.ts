import {mkdir,stat} from 'node:fs/promises';
import {join,isAbsolute} from 'node:path';
import {BACKGROUND_CONTEXT as context} from '@earendil-works/chord/context';
import {defineDoc,defineExtension,defineTask,type JsonObject,type TaskId} from '@earendil-works/pi-durable';
import {importRustLauncher,type ImportedLauncherIdentity} from '../../nautilus/src/rust-launcher-import.ts';
import {buildRustLauncher} from '../../nautilus/src/rust-launcher-build.ts';
import type {ResearchHost} from '../../research/src/host.ts';
import type {NativeRustEnvironment,ProcessNativeTarget} from '../../../packages/contracts/src/nautilus.ts';
import {validateNativeTarget} from '../../research/src/native-execution.ts';
import {canonical} from '../../research/src/documents.ts';
import {object,Problem,json} from './facts.ts';
import type {ServiceConfig} from './config.ts';
import type {instanceSettings} from './instance-settings.ts';

const BuildRequests=defineDoc<{requests:Record<string,{taskId:number,input:JsonObject}>}>({kind:'qz.native-environment-builds',version:1,scope:'session',initial:()=>({requests:{}}),checkpointWhen:()=>true});
type ImportSource={environmentId:string;sourceExecutable:string;expectedIdentity?:ImportedLauncherIdentity};
type Input={requestId:string;revision:number;environment:NativeRustEnvironment|null;importSource?:ImportSource};
/** Public categories only: never forward compiler output, file paths or arbitrary errors. */
export function nativePreparationFailure(error:unknown):string {
 const code=(error as {code?:unknown}|null)?.code;
 if(code==='ENOENT')return 'native_preparation_required_file_unavailable';
 if(code==='EACCES'||code==='EPERM')return 'native_preparation_file_access_denied';
 const message=error instanceof Error?error.message:'';
 if(message==='rust_import_identity_mismatch'||message==='rust_build_binary_identity_mismatch')return 'native_preparation_identity_mismatch';
 if(message==='rust_import_capabilities_invalid'||message==='rust_build_capabilities_mismatch')return 'native_preparation_capabilities_invalid';
 return 'native_build_not_verified';
}
async function preparationDiagnostics(directory:string){
 const files:string[]=[];
 for(const name of ['build.log','capabilities.stdout','capabilities.stderr']){
  try{if((await stat(join(directory,name))).isFile())files.push(name);}catch{/* Absence/access failure is not evidence of an available log. */}
 }
 return files;
}
/** Original official Pi task owns construction and uncertainty. No parallel job ledger. */
export function nativeEnvironmentBuild(config:ServiceConfig,build=buildRustLauncher,importRunner=importRustLauncher){
 const Build=defineTask<Input,{phase:'compile'|'unknown'},JsonObject>({name:'qz.native-environment-build',version:1,initial:()=>({phase:'compile'}),phases:{
  async compile(task,runtime,ctx){
   // A lost build may have produced files; never automatically compile it twice.
   await runtime.commit(()=>({status:'running',checkpoint:{phase:'unknown'}}),ctx);
   const root=join(config.dataDir,'native-builds');await mkdir(root,{recursive:true});
   const selected=task.input.environment;
   try{
    if(!task.input.importSource&&!selected)throw new Error('rust_environment_not_configured');
    const result=task.input.importSource?await importRunner({sourceExecutable:task.input.importSource.sourceExecutable,expectedIdentity:task.input.importSource.expectedIdentity,directory:join(root,String(task.id)),signal:runtime.signal}):await build({source:selected!,factories:{backtestFactory:selected!.backtestFactory,accountFactory:selected!.accountFactory,accountCompatibility:selected!.accountCompatibility},glueSourceRoot:process.env.QZ_NATIVE_GLUE_SOURCE??'/opt/quazonai/native-glue',directory:join(root,String(task.id)),signal:runtime.signal});
    const target=validateNativeTarget({kind:'RUST_PROCESS',id:task.input.importSource?.environmentId??selected!.id,binary:result.executable,capabilities:result.capabilities}) as ProcessNativeTarget;
    await runtime.commit(()=>({status:'terminal',outcome:{status:'completed',result:json({qualification:result.qualification,target,...('manifestPath' in result?{manifestPath:result.manifestPath}:{}),connected:false,strategyStarted:false})}}),ctx);
   }catch(error){
    if(runtime.signal.aborted)throw error;
    // Early preflight can fail before a task directory exists. Name only logs verified present.
    const diagnosticFiles=await preparationDiagnostics(join(root,String(task.id)));
    await runtime.commit(()=>({status:'terminal',outcome:{status:'failed',error:{message:nativePreparationFailure(error),detail:{diagnosticFiles}}}}),ctx);
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
  async function start(value:unknown,importing=false){
   const body=object(value);if(Object.keys(body).some(k=>!(importing?['requestId','expectedRevision','environmentId','sourceExecutable','expectedIdentity']:['requestId','expectedRevision']).includes(k))||typeof body.requestId!=='string'||!body.requestId.trim()||!Number.isSafeInteger(body.expectedRevision))throw new Problem('native_build_request_invalid',422);
   let importSource:ImportSource|undefined;
   if(importing){
    if(typeof body.environmentId!=='string'||!body.environmentId.trim()||typeof body.sourceExecutable!=='string'||!isAbsolute(body.sourceExecutable))throw new Problem('native_import_source_invalid',422);
    let expectedIdentity:ImportedLauncherIdentity|undefined;
    if(body.expectedIdentity!==undefined){const expected=object(body.expectedIdentity);if(Object.keys(expected).some(k=>!['nautilusSource','nautilusVersion','precision'].includes(k))||typeof expected.nautilusSource!=='string'||!expected.nautilusSource||typeof expected.nautilusVersion!=='string'||!expected.nautilusVersion||typeof expected.precision!=='string'||!['standard','high'].includes(expected.precision))throw new Problem('native_import_expected_identity_invalid',422);expectedIdentity=expected as ImportedLauncherIdentity;}
    importSource={environmentId:body.environmentId,sourceExecutable:body.sourceExecutable,...(expectedIdentity?{expectedIdentity}: {})};
   }
   const current=settings.view();const saved=(await host.harness.snapshot(BuildRequests,context))?.requests[body.requestId];
   if(saved){if(saved.input.revision!==body.expectedRevision||canonical(saved.input.importSource??null)!==canonical(json(importSource??null)))throw new Problem('request_id_conflict',409);return read(body.requestId);}
   if(current.revision!==body.expectedRevision)throw new Problem('instance_settings_revision_conflict',409);
   if(!current.environment&&!importing)throw new Problem('rust_environment_not_configured',409);
   if(importSource&&current.environment&&current.environment.id!==importSource.environmentId)throw new Problem('native_import_environment_mismatch',409);
   const input:Input={requestId:body.requestId,revision:current.revision,environment:current.environment??null,...(importSource?{importSource}:{})};
   const root=await host.harness.root(context);
   await root.commit(async tx=>{const requests=await tx.doc(BuildRequests);const prior=requests.requests[input.requestId];if(prior){if(canonical(prior.input)!==canonical(json(input)))throw new Problem('request_id_conflict',409);return;}
    const taskId=await tx.createTask(Build,input,{ownership:{kind:'conversation'},background:true});requests.requests[input.requestId]={taskId,input:json(input)};
   },context);return read(input.requestId);
  }
  async function select(value:unknown){
   const body=object(value);if(typeof body.requestId!=='string'||!Number.isSafeInteger(body.expectedRevision)||Object.keys(body).some(k=>!['requestId','expectedRevision'].includes(k)))throw new Problem('native_build_selection_invalid',422);
   const result=await read(body.requestId);const current=settings.view();
   if(body.expectedRevision!==result.input.revision||canonical(result.input.environment as never)!==canonical((current.environment??null) as never))throw new Problem('native_build_environment_changed',409);
   if(result.state.status!=='terminal'||result.state.outcome.status!=='completed')throw new Problem('native_build_not_completed',409);
   const target=validateNativeTarget(result.state.outcome.result.target) as ProcessNativeTarget;
   return settings.selectRunner(Number(body.expectedRevision),target);
  }
  async function selectedTarget(requestId:string, expectedRevision:number) {
   const result=await read(requestId);const current=settings.view();
   if(!Number.isSafeInteger(expectedRevision)||current.revision!==expectedRevision)throw new Problem('instance_settings_revision_conflict',409);
   if(result.state.status!=='terminal'||result.state.outcome.status!=='completed')throw new Problem('native_build_not_completed',409);
   const target=validateNativeTarget(result.state.outcome.result.target) as ProcessNativeTarget;
   if(!current.runner||canonical(current.runner as never)!==canonical(target as never))throw new Problem('native_build_runner_not_selected',409);
   if(!target.capabilities.research.configuredRunFactory)throw new Problem('native_runner_factory_not_available',409);
   return target;
  }
  function configuration() {
   const current=settings.view();const environment=current.environment;
   return {revision:current.revision,environment:environment?{id:environment.id,packageName:environment.packageName,backtestFactory:environment.backtestFactory??null}:null,
    selectedRunner:current.runner?{id:current.runner.id,capabilities:current.runner.capabilities}:null,connected:false,strategyStarted:false};
  }
  return {configuration,start:(value:unknown)=>start(value),startImport:(value:unknown)=>start(value,true),read,select,selectedTarget};
 }
 return {extension,workbench};
}
