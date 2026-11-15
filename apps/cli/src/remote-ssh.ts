import {isAbsolute} from 'node:path';
import {CliError} from './profile.ts';

/** Only internal callers supply the fixed, validated remote command. No user's shell string. */
export function remoteSshArguments(input:Record<string,unknown>,command:string):string[]{
 if(typeof input.host!=='string'||!/^[a-zA-Z0-9][a-zA-Z0-9.:-]*$/.test(input.host)
  ||typeof input.user!=='string'||!/^[a-zA-Z_][a-zA-Z0-9_-]*$/.test(input.user))throw new CliError('remote_owner_destination_invalid');
 const port=input.port??22;
 if(!Number.isSafeInteger(port)||Number(port)<1||Number(port)>65535)throw new CliError('remote_owner_port_invalid');
 if(typeof input.knownHostsFile!=='string'||!isAbsolute(input.knownHostsFile)||/[\s"'\\%~\x00-\x1f]/.test(input.knownHostsFile))throw new CliError('remote_owner_known_hosts_invalid');
 return ['-F','none','-T','-o','BatchMode=yes','-o','StrictHostKeyChecking=yes','-o','UpdateHostKeys=no',
  '-o',`UserKnownHostsFile=${input.knownHostsFile}`,'-o','GlobalKnownHostsFile=none',
  '-o','ForwardAgent=no','-o','ClearAllForwardings=yes','-o','PermitLocalCommand=no',
  '-o','ControlMaster=no','-o','ControlPath=none','-o','ConnectionAttempts=1','-o','ConnectTimeout=10',
  '-p',String(port),'-l',input.user,'--',input.host,command];
}
export function remoteSshEnvironment():NodeJS.ProcessEnv{return {PATH:process.env.PATH,HOME:process.env.HOME,USERPROFILE:process.env.USERPROFILE,SYSTEMROOT:process.env.SYSTEMROOT,SSH_AUTH_SOCK:process.env.SSH_AUTH_SOCK};}
export function remoteAbsolutePath(value:unknown):value is string{return typeof value==='string'&&/^\/[a-zA-Z0-9_./-]+$/.test(value)&&!value.split('/').includes('..');}
