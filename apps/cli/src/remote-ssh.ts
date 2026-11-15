import {remoteSshArguments as argumentsFor} from '../../nautilus/src/remote-account-ssh.ts';
import {CliError} from './profile.ts';
export {remoteSshEnvironment,remoteAbsolutePath} from '../../nautilus/src/remote-account-ssh.ts';
export function remoteSshArguments(input:Record<string,unknown>,command:string):string[]{
 try{return argumentsFor(input,command);}catch(error){throw new CliError(error instanceof Error&&/^remote_owner_[a-z_]+$/.test(error.message)?error.message:'remote_owner_configuration_invalid');}
}
