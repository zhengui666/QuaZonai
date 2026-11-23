// Standalone pure-glue entry; no build tool, native library or strategy loading.
import {readFile} from 'node:fs/promises';
const args=process.argv.slice(2);
if(args.length===1&&args[0]==='--version'){
 const {version}=JSON.parse(await readFile(new URL('../distribution.json',import.meta.url),'utf8'));
 process.stdout.write(`quazonai-launcher ${version}\n`);
}else if(args.length===0||args.length===1&&['--help','help'].includes(args[0])){
 process.stdout.write('quazonai-launcher inspect|hello|request|serve --file CONFIG.json\nquazonai-launcher relay --socket-path PRIVATE_SOCKET\n\nPure QZ glue with bundled Node. No Nautilus, strategies or compilation.\nRequires an already installed trusted compatible owner; arbitrary Nautilus is not enough.\nserve/relay require Linux; remote SSH clients require existing pinned host trust.\nSaving configuration or inspecting does not request trading START.\nSee LAUNCHER.md for the exact protocol and lifecycle restrictions.\n');
}else if(args[0]==='remote-account'&&args[1]==='relay'){
 // The existing SSH client uses this fixed wire invocation for both packages.
 await import('./apps/cli/src/main.ts');
}else if(['inspect','hello','request','serve','relay'].includes(args[0])){
 process.argv.splice(2,0,args[0]==='inspect'?'remote-owner':'remote-account');
 await import('./apps/cli/src/main.ts');
}else{
 process.stderr.write('{"error":"launcher_command_invalid"}\n');process.exitCode=1;
}
