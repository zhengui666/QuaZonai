import {spawn} from 'node:child_process';
import {open} from 'node:fs/promises';
import {join} from 'node:path';

/** Linux flock is attached to the inherited open file description, not a PID file.
 * Keep the parent descriptor open for the entire receipt owner's lifetime. Kernel
 * close/crash releases it; never unlink the inode or steal another owner's lock.
 * This protects one directory, not a signer shared across directories or hosts.
 */
export async function acquireAccountReceiptLock(directory:string):Promise<{fd:number;close():Promise<void>}> {
 if(process.platform!=='linux')throw new Error('account_receipts_lock_platform_unsupported');
 const file=await open(join(directory,'capital-receipts.lock'),'a+',0o600);
 try {
  await new Promise<void>((resolve,reject)=>{
   const child=spawn('/usr/bin/flock',['--nonblock','--exclusive','3'],{stdio:['ignore','ignore','ignore',file.fd],env:{PATH:'/usr/bin:/bin'}});
   child.once('error',()=>reject(new Error('account_receipts_lock_unavailable')));
   child.once('exit',(code)=>code===0?resolve():reject(new Error(code===1?'account_receipts_owner_already_running':'account_receipts_lock_unavailable')));
  });
 }catch(error){await file.close();throw error;}
 return {fd:file.fd,close:()=>file.close()};
}
