import {test} from 'node:test';
import assert from 'node:assert/strict';
import {spawn,type ChildProcess} from 'node:child_process';
import {once} from 'node:events';
import {mkdtemp,readFile,writeFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {createCapitalAccount} from '../src/capital-account.ts';
import {startRemoteAccountSupervisor} from '../src/remote-account-supervisor.ts';
import type {AccountOptions} from '../src/account-runtime.ts';

// CI only. These children are descriptor/lifecycle fixtures, not Nautilus and not
// evidence that an arbitrary native owner survives signals, EOF or controller death.
const options:AccountOptions={id:'paper',clientId:'client',accountId:'account',mode:'paper',instruments:['TEST.SIM'],startingBalance:'100',paperFeeRate:'0.001',nativeStrategy:{instrument_id:'TEST.SIM',strategy:{kind:'REGISTERED',name:'owner',config:null}}};
const pause=()=>new Promise(resolve=>setTimeout(resolve,20));
async function eventually(check:()=>Promise<boolean>){for(let n=0;n<150;n++){if(await check())return;await pause();}throw Error('fixture_condition_timeout');}
async function ready(child:ChildProcess):Promise<{pid:number}>{
 return new Promise((resolve,reject)=>{let buffer='';const timer=setTimeout(()=>finish(Error('fixture_start_timeout')),5000);
 const finish=(error?:Error,value?:{pid:number})=>{clearTimeout(timer);child.stdout?.off('data',data);child.off('exit',exit);error?reject(error):resolve(value!);};
 const data=(chunk:Buffer)=>{buffer+=chunk.toString();if(buffer.includes('\n')){try{finish(undefined,JSON.parse(buffer.split('\n')[0]!));}catch{finish(Error('fixture_bad_ready'));}}};
 const exit=()=>finish(Error('fixture_exited_before_ready'));child.stdout!.on('data',data);child.once('exit',exit);child.once('error',error=>finish(error));
 });
}
for(const survives of [true,false])test(`supervisor abnormal exit preserves receipts; fixture owner survives=${survives}`,{timeout:20000,skip:process.platform!=='linux'},async()=>{
 const root=await mkdtemp(join(tmpdir(),'qz-remote-orphan-')),directory=join(root,'account');let controller:ChildProcess|undefined,ownerPid:number|undefined;
 try{
  const seed=await createCapitalAccount(options,directory,'/never-execute');await seed.close();
  const receiptPath=join(directory,'capital-receipts.json');const saved=JSON.parse(await readFile(receiptPath,'utf8'));
  saved.strategyCommands={original:{original:{action:'START',requestId:'original'},receipt:{status:'UNCERTAIN',error:'original_unknown'}}};
  await writeFile(receiptPath,JSON.stringify(saved));const before=await readFile(receiptPath,'utf8');
  const accountSource=new URL('../src/capital-account.ts',import.meta.url).href,supervisorSource=new URL('../src/remote-account-supervisor.ts',import.meta.url).href;
  // The fixture discovers the lock descriptor solely in its own process. No
  // production API exposes a descriptor and no real owner/credentials are used.
  const fixture=`process.stdin.resume();${survives?"setInterval(()=>{},1000);process.on('SIGINT',()=>{});process.on('SIGTERM',()=>{});":"process.stdin.on('end',()=>process.exit(0));"}`;
  const code=`import {spawn} from 'node:child_process';import {readdir,readlink} from 'node:fs/promises';import {createCapitalAccount} from ${JSON.stringify(accountSource)};import {startRemoteAccountSupervisor} from ${JSON.stringify(supervisorSource)};
const account=await createCapitalAccount(${JSON.stringify(options)},${JSON.stringify(directory)},'/never-execute');
const supervisor=await startRemoteAccountSupervisor({socketDirectory:${JSON.stringify(join(root,'original-socket'))},runnerIdentity:'fixture',clientId:'client',accountId:'account',openAccount:async()=>account});
let fd;for(const name of await readdir('/proc/self/fd')){try{if(await readlink('/proc/self/fd/'+name)===${JSON.stringify(join(directory,'capital-receipts.lock'))})fd=Number(name);}catch{}}
if(fd===undefined)throw Error('fixture_lock_missing');const owner=spawn(process.execPath,['--input-type=module','--eval',${JSON.stringify(fixture)}],{stdio:['pipe','ignore','ignore',fd]});await new Promise((resolve,reject)=>{owner.once('spawn',resolve);owner.once('error',reject);});console.log(JSON.stringify({pid:owner.pid}));process.stdin.resume();`;
  controller=spawn(process.execPath,['--experimental-strip-types','--input-type=module','--eval',code],{stdio:['pipe','pipe','pipe'],env:{PATH:process.env.PATH}});
  ownerPid=(await ready(controller)).pid;assert.ok(Number.isInteger(ownerPid));
  const exited=once(controller,'exit');controller.kill('SIGKILL');await exited;
  const reopen=()=>startRemoteAccountSupervisor({socketDirectory:join(root,'replacement-'+Math.random().toString(16).slice(2)),runnerIdentity:'fixture',clientId:'client',accountId:'account',openAccount:()=>createCapitalAccount(options,directory,'/never-execute')});
  if(survives){
   await assert.rejects(reopen(),/account_receipts_owner_already_running/);
   assert.equal(await readFile(receiptPath,'utf8'),before,'competing supervisor must not overwrite original UNKNOWN');
   process.kill(ownerPid,'SIGKILL');ownerPid=undefined;
  }
  // A normal EOF-exiting fixture may release the lock. That says nothing about
  // an exchange outcome: reopening must retain UNKNOWN and must not replay it.
  let replacement:Awaited<ReturnType<typeof reopen>>|undefined;
  await eventually(async()=>{try{replacement=await reopen();return true;}catch(error){if(!(error instanceof Error)||!error.message.includes('account_receipts_owner_already_running'))throw error;return false;}});
  assert.ok(replacement);
  try{const retained=JSON.parse(await readFile(receiptPath,'utf8'));assert.deepEqual(retained.strategyCommands,saved.strategyCommands);assert.equal(retained.sourceId,saved.sourceId);}
  finally{await replacement.shutdownOwner();}
 }finally{controller?.kill('SIGKILL');if(ownerPid)try{process.kill(ownerPid,'SIGKILL');}catch{}await rm(root,{recursive:true,force:true});}
});
