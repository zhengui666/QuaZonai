import {mkdir,mkdtemp,open,readFile,readdir,rename,rm} from 'node:fs/promises';
import {join} from 'node:path';
import {createCapitalAccount} from '../../nautilus/src/capital-account.ts';
import {accountCreation,physicalAccount,saveAccountConfiguration,sameAccountCreation,type AccountCreation} from '../../nautilus/src/account-configurations.ts';
import {validateNativeTarget} from '../../research/src/native-execution.ts';
import type {ProcessNativeTarget} from '../../../packages/contracts/src/nautilus.ts';
import type {ServiceConfig,ConnectionConfig} from './config.ts';
import {registerProcessAccount} from './downstream.ts';
import {Problem,object,text} from './facts.ts';

type Binding={schemaVersion:1;creation:AccountCreation;runner:ProcessNativeTarget};
type OpenAccount={binding:Binding;connection:ConnectionConfig;account:Awaited<ReturnType<typeof createCapitalAccount>>;detach:()=>void};
/** Immutable process provenance beside the existing account files, not a new OMS. */
async function syncDirectory(path:string){const handle=await open(path,'r');try{await handle.sync();}finally{await handle.close();}}
export async function processAccounts(config:ServiceConfig,publishDirectory:typeof rename=rename,syncPublication:typeof syncDirectory=syncDirectory){
 const historical:AccountCreation[]=[];let historicalIdentityUnknown=false;
 const root=join(config.dataDir,'accounts');await mkdir(root,{recursive:true});
 const opened=new Map<string,OpenAccount>();const retained=new Map<string,Binding>();const unavailable=new Map<string,string>();let lane=Promise.resolve();let closing=false;
 function validate(value:unknown):Binding{
  const saved=value as Binding;if(saved?.schemaVersion!==1)throw new Problem('native_account_binding_invalid',409);
  const runner=validateNativeTarget(saved.runner);if(!('kind' in runner))throw new Problem('native_account_process_binding_required',409);
  return {schemaVersion:1,creation:accountCreation(saved.creation),runner};
 }
 function ensureCapability(binding:Binding){
  if(!binding.runner.capabilities.account.compiled)throw new Problem('native_runner_account_not_compiled',409);
  if(binding.creation.configuration.nativeStrategy.strategy.kind==='REGISTERED'&&!binding.runner.capabilities.account.registration)throw new Problem('native_runner_account_registration_missing',409);
 }
 function ensureUnique(binding:Binding){
  const options=binding.creation.configuration;
  // A legacy URL does not prove which venue identity it owns. Never start a second
  // live node by assuming an unknown old identity is unrelated to the new address.
  if(config.connections.some(c=>c.kind!=='RUST_PROCESS'&&(c.id===options.id||c.accountId===options.accountId||options.mode==='live'&&c.mode==='live')))throw new Problem('historical_account_identity_requires_explicit_reconciliation',409);
  if(historicalIdentityUnknown&&options.mode==='live'||historical.some(old=>physicalAccount(old.configuration)===physicalAccount(options)))throw new Problem('historical_account_identity_requires_explicit_reconciliation',409);
  for(const [id,other] of retained)if(id!==options.id&&physicalAccount(other.creation.configuration)===physicalAccount(options))throw new Problem('native_physical_account_already_configured',409);
 }
 async function attach(binding:Binding){
  const options=binding.creation.configuration;ensureCapability(binding);ensureUnique(binding);
  const directory=join(root,options.id);
  await syncPublication(root);
  const account=await createCapitalAccount(options,directory,binding.runner.binary);
  const connection:ConnectionConfig={id:options.id,name:options.id,mode:options.mode,clientId:options.clientId,accountId:options.accountId,
   url:`rust-process://local/${encodeURIComponent(binding.runner.id)}/${encodeURIComponent(options.id)}`,kind:'RUST_PROCESS',processBinding:{runner:binding.runner,directory}};
  try{const detach=registerProcessAccount(connection,account.request);opened.set(options.id,{binding,connection,account,detach});config.connections.push(connection);}
  catch(error){await account.close();throw error;}
 }
 async function closeOpened(){
  const items=[...opened.values()];opened.clear();
  for(const item of items){item.detach();config.connections=config.connections.filter(connection=>connection!==item.connection);}
  const outcomes=await Promise.allSettled(items.map(item=>item.account.close()));
  const failed=outcomes.find((value):value is PromiseRejectedResult=>value.status==='rejected');if(failed)throw failed.reason;
 }
 try {
 for(const entry of await readdir(root,{withFileTypes:true})){
  if(!entry.isDirectory()||entry.name.startsWith('.creating-'))continue;let raw:string;try{raw=await readFile(join(root,entry.name,'process-binding.json'),'utf8');}catch(error){if((error as NodeJS.ErrnoException).code==='ENOENT'){try{historical.push(accountCreation(JSON.parse(await readFile(join(root,entry.name,'configuration.json'),'utf8'))));}catch{historicalIdentityUnknown=true;}continue;}throw error;}
  const binding=validate(JSON.parse(raw));if(binding.creation.configuration.id!==entry.name)throw new Problem('native_account_binding_directory_mismatch',409);retained.set(entry.name,binding);
 }
 // Only fully saved original configurations are opened, disconnected. Partial saves
 // require the original explicit creation request; legacy directories are never claimed.
 for(const binding of retained.values()){
  try{const saved=accountCreation(JSON.parse(await readFile(join(root,binding.creation.configuration.id,'configuration.json'),'utf8')));if(!sameAccountCreation(saved,binding.creation))throw new Problem('native_account_configuration_conflict',409);await attach(binding);}
  catch(error){if((error as NodeJS.ErrnoException).code==='ENOENT')continue;if(error instanceof Problem&&error.message==='historical_account_identity_requires_explicit_reconciliation'){unavailable.set(binding.creation.configuration.id,error.message);continue;}throw error;}
 }
 }catch(error){await closeOpened();throw error;}
 async function list():Promise<Array<Record<string,unknown>&{id:string}>>{return Promise.all([...retained.values()].map(async binding=>{const id=binding.creation.configuration.id,item=opened.get(id);return {id,accountConfiguration:binding.creation.configuration,environmentId:binding.runner.id,configurationOnly:true,automaticConnection:false,automaticStrategyStart:false,
  ...(item?(await item.account.request('/strategy/config','GET',item.connection.clientId)).value as object:{unavailable:unavailable.get(id)??'original_creation_not_fully_saved'})};}));}
 function create(value:unknown){
  const creation=accountCreation(value);
  const work=lane.then(async()=>{
   if(closing)throw new Problem('service_closing',503);
   const id=creation.configuration.id,prior=retained.get(id);
   if(prior&&!sameAccountCreation(prior.creation,creation))throw new Problem('native_account_configuration_conflict',409);
   if(opened.has(id))return (await list()).find(item=>item.id===id)!;
   let binding=prior;
   if(!binding){
    if(!config.nativeProcess)throw new Problem('native_runner_not_selected',409);
    binding={schemaVersion:1,creation,runner:structuredClone(config.nativeProcess)};ensureCapability(binding);ensureUnique(binding);
    // Any pre-existing account data without a verified process binding is historical.
    try{await readdir(join(root,id));throw new Problem('historical_account_directory_requires_explicit_reconciliation',409);}catch(error){if((error as NodeJS.ErrnoException).code!=='ENOENT')throw error;}
    // Assemble only new configuration in a separate temporary directory. No source
    // ID, receipts, cache or process exists here. Publish the complete identity in
    // one directory rename, never leave an empty final path looking historical.
    const staging=await mkdtemp(join(root,'.creating-'));
    try{
     await saveAccountConfiguration(staging,creation);
     const directory=join(staging,id);
     const file=await open(join(directory,'process-binding.json'),'wx',0o600);try{await file.writeFile(JSON.stringify(binding));await file.sync();}finally{await file.close();}
     const prepared=await open(directory,'r');try{await prepared.sync();}finally{await prepared.close();}
     await publishDirectory(directory,join(root,id));retained.set(id,binding);
     await syncPublication(root);
    }finally{await rm(staging,{recursive:true,force:true});}

   }
   await saveAccountConfiguration(root,creation);await syncPublication(root);await attach(binding);return (await list()).find(item=>item.id===id)!;
  });lane=work.then(()=>undefined,()=>undefined);return work;
 }
 async function binding(id:string){const item=opened.get(id);if(!item)throw new Problem('native_process_original_account_unavailable',409);const saved=object((await item.account.request('/strategy/config','GET',item.connection.clientId)).value);return {connection:structuredClone(item.connection),view:{currency:text(saved.currency,'account_currency')}};}
 return {list,create,binding,async close(){closing=true;await lane;await closeOpened();}};
}
