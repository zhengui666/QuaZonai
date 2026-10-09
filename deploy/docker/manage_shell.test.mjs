import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdir, mkdtemp, readFile, readlink, rm, symlink, writeFile } from 'node:fs/promises';
import { tmpdir, userInfo } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
const bundle=fileURLToPath(new URL('./',import.meta.url));
const quote=s=>`'${s.replaceAll("'", "'\\''")}'`;
async function fixture(t){const root=await mkdtemp(join(tmpdir(),'quazonai-shell-state-'));t.after(()=>rm(root,{recursive:true,force:true}));await mkdir(join(root,'work'));return root;}
function run(root,body){return spawnSync('bash',['-c',`source ${quote(join(bundle,'manage.sh'))}\nQZ_WORK=${quote(join(root,'work'))}\n${body}`],{encoding:'utf8',timeout:20000});}
function success(result){assert.equal(result.status,0,result.stderr);return result.stdout;}
const digest='a'.repeat(64),revision='b'.repeat(40);
function release(version){return {schema_version:2,version,revision,image:`ghcr.io/zhengui666/quazonai@sha256:${digest}`,runtime_image:`ghcr.io/zhengui666/quazonai-runtime@sha256:${digest}`,codex_image:`ghcr.io/zhengui666/quazonai-codex@sha256:${digest}`,codex_version:'0.157.0',database_image:'ghcr.io/pgmq/pg18-pgmq@sha256:bfb3537068ce453609744518ece92b178ac89dff53747d47ca6fab91c2fc66a6'};}
async function installation(root){const home=join(root,'home 中文 % [x]'),dir=join(home,'cluster');await mkdir(dir,{recursive:true});return {...release('v1.0.0'),root:dir,uid:userInfo().uid,gid:userInfo().gid,home,codex_home:join(home,'codex'),unit_directory:join(home,'units'),path:'/usr/bin:/bin',port:8081,database_port:55432,password:'c'.repeat(64),project:'quazonai-fixture',bundle:join(dir,'releases/v1.0.0/deployment'),docker_socket:'/var/run/docker.sock',docker_socket_gid:1000,codex_runtime_image:'quazonai-codex:quazonai-fixture',runtime_targets:[{origin:'https://运行.example',addresses:['127.0.0.1:443']}],downstream_targets:[],unknown:{keep:'exact'}};}
test('strict JSON preserves old escaped Unicode and exact numbers; malformed data fails',async t=>{
 const root=await fixture(t),file=join(root,'state.json');await writeFile(file,'{"root":"/home/\\u996d\\u996d/\\ud83d\\ude80 [x] %","large":900719925474099399999}');
 assert.equal(success(run(root,`qz_get ${quote(file)} root`)),'/home/饭饭/🚀 [x] %');assert.match(success(run(root,`qz_raw ${quote(file)} ''`)),/900719925474099399999/);
 for(const value of ['{"a":1,"\\u0061":2}','{"n":01}','{"n":+1}','{"n":1.}','{"n":NaN}','{"x":"\\ud800"}','{} garbage','{"x":"line\nbreak"}']){await writeFile(file,value);assert.notEqual(run(root,`qz_raw ${quote(file)} ''`).status,0,value);}
 for(const value of ['{"x":"\\u0000"}','{"x":"bad\\n"}']){await writeFile(file,value);assert.notEqual(run(root,`qz_get ${quote(file)} x`).status,0);}
});
test('missing fields and failed JSON merge preserve original bytes',async t=>{
 const root=await fixture(t),file=join(root,'old.json'),bad=join(root,'bad.json'),patch=join(root,'patch.json');await writeFile(file,'{"preserve":"original"}\n');await writeFile(bad,'{"broken":');await writeFile(patch,'{"new":true}');
 assert.notEqual(run(root,`qz_get ${quote(file)} missing; echo SURVIVED`).status,0);assert.notEqual(run(root,`qz_merge ${quote(bad)} ${quote(patch)} ${quote(file)}`).status,0);assert.equal(await readFile(file,'utf8'),'{"preserve":"original"}\n');
 assert.notEqual(run(root,`printf '{"broken":' | qz_atomic_text ${quote(file)}`).status,0);assert.equal(await readFile(file,'utf8'),'{"preserve":"original"}\n');
});
test('canonical merge retains unknown fields and keeps shell metacharacters inert',async t=>{
 const root=await fixture(t),file=join(root,'old.json'),patch=join(root,'patch.json'),value={path:'/中文/$(touch NEVER); `echo no`',unknown:{array:[1,true,null],text:'a\nb'},version:'v1.0.0'};await writeFile(file,JSON.stringify(value));await writeFile(patch,'{"version":"v2.0.0"}');success(run(root,`qz_merge ${quote(file)} ${quote(patch)} ${quote(file)}`));assert.deepEqual(JSON.parse(await readFile(file,'utf8')),{...value,version:'v2.0.0'});assert.equal(success(run(root,`stat -c %a ${quote(file)}`)).trim(),'600');
});
test('SemVer comparison does not round large decimal release IDs',async t=>{const root=await fixture(t);for(const [a,b,v] of [['v1.0.0-dev.99','v1.0.0-dev.100',-1],['v1.0.0','v1.0.0-dev.1',1],['v1.0.0-1','v1.0.0-a',-1],['v1.0.0-dev.999999999999999999999999','v1.0.0-dev.1000000000000000000000000',-1],['v1.0.0-a-b','v1.0.0-a-c',-1]])assert.equal(success(run(root,`qz_precedence ${quote(a)} ${quote(b)}`)).trim(),String(v));});
test('missing password fails before any Worker environment is produced',async t=>{const root=await fixture(t),config=await installation(root),file=join(root,'config.json');delete config.password;await writeFile(file,JSON.stringify(config));const result=run(root,`qz_worker_environment ${quote(file)}`);assert.notEqual(result.status,0);assert.equal(result.stdout,'');});
test('source mount trailing-slash symlinks fail while owner Unicode directories work',async t=>{const root=await fixture(t),config=await installation(root),file=join(root,'config.json'),input=join(root,'input 中文');await writeFile(file,JSON.stringify(config));await mkdir(input);await symlink(input,join(root,'link'));assert.notEqual(run(root,`qz_source_path ${quote(join(root,'link')+'/')} ${quote(file)} false`).status,0);assert.equal(success(run(root,`qz_source_path ${quote(input)} ${quote(file)} false`)),input);});
async function lifecycle(t,phase){const root=await fixture(t),old=await installation(root),target={...old,...release('v1.0.1'),bundle:join(old.root,'releases/v1.0.1/deployment')};await writeFile(join(old.root,'installation.json'),JSON.stringify(old));await writeFile(join(root,'candidate.json'),JSON.stringify(target));await symlink(join(old.root,'releases',old.version),join(old.root,'current'));
 const retained=new Map([
  [join(old.root,'data/state/master.key'),'fixture-original-key'],[join(old.root,'.env'),'CODEX_VERSION=0.157.0\n'],[join(old.root,'worker.env'),'fixture-prepared-worker'],
  [join(old.root,'backups/original/installation.json'),JSON.stringify(old)],[join(old.root,'backups/original/database.dump'),'fixture-original-dump'],
  [join(root,'runtime/stopped.json'),JSON.stringify({fixture:true,stopped:true,image:old.runtime_image,catalog:'original'})],[join(root,'runtime/credential'),'fixture-original-runtime-credential'],[join(root,'runtime/journal'),'fixture-original-runtime-journal'],
 ]);for(const [name,value] of retained){await mkdir(dirname(name),{recursive:true});await writeFile(name,value);}
 if(phase)await writeFile(join(old.root,'pending.json'),JSON.stringify({operation:'update',phase,previous:old,target,backup:phase==='preparing'?null:join(old.root,'backups/original')}));const library=`
ROOT=${quote(old.root)}
LOG=${quote(join(root,'calls'))}
qz_preflight() { :; }
qz_prepare() { echo prepare >> "$LOG"; QZ_PREPARED=${quote(join(root,'candidate.json'))}; }
qz_manifest() { :; }
qz_get() { if [[ "$1" == "$QZ_BUNDLE/release.json" && "$2" == version ]]; then printf v1.0.1; else LC_ALL=C awk -v mode=get -v path="$2" -f "$QZ_BUNDLE/json.awk" "$1"; fi; }
qz_require_idle() { echo idle >> "$LOG"; }
qz_codex_require_stopped() { echo codex-stopped >> "$LOG"; }
qz_restarts() { echo "restarts $2" >> "$LOG"; }
qz_configure_worker() { echo configure-worker >> "$LOG"; }
systemctl() { echo "systemctl $*" >> "$LOG"; if [[ " $* " == *' --property=MainPID '* ]]; then printf '123\\n'; fi; }
sleep() { :; }
curl() { echo "curl $*" >> "$LOG"; if [[ "$*" == *'/health/live' ]]; then printf 204; else printf '<!doctype html>fixture' > "$QZ_WORK/frontend.html"; fi; }
docker() { echo unexpected-docker >> "$LOG"; return 99; }
qz_compose() { echo "processor-version $(qz_get "$1" version)" >> "$LOG"; shift; echo "compose $*" >> "$LOG"; if [[ " $* " == *' migrate '* ]]; then [[ $(qz_get "$ROOT/pending.json" phase) == migrating ]]; fi; }
qz_backup() { echo backup >> "$LOG"; printf '%s\\n' "$ROOT/backups/fixture" > "$QZ_WORK/recovery-path"; }
`;return {root,old,target,library,retained,log:()=>readFile(join(root,'calls'),'utf8')};}
test('starting retry resumes exact prepared processors without migration or Worker rewrite',async t=>{const f=await lifecycle(t,'starting');success(run(f.root,f.library+'qz_apply_update "$ROOT"'));const log=await f.log();assert.match(log,/compose up -d --no-recreate --wait --wait-timeout 120 app/);assert.match(log,/systemctl --user start/);assert.match(log,/restarts true/);assert.doesNotMatch(log,/migrate|configure-worker|compose stop|backup/);assert.equal(JSON.parse(await readFile(join(f.old.root,'installation.json'),'utf8')).version,'v1.0.1');});
test('normal update durably backs up before migration and preserves original settings',async t=>{const f=await lifecycle(t);success(run(f.root,f.library+'qz_apply_update "$ROOT"'));const log=await f.log();assert.ok(log.indexOf('backup')<log.indexOf('compose run --rm --no-deps app migrate'));assert.ok(log.indexOf('configure-worker')<log.indexOf('systemctl --user start'));const result=JSON.parse(await readFile(join(f.old.root,'installation.json'),'utf8'));for(const key of ['password','project','runtime_targets','downstream_targets','unknown','root','codex_home'])assert.deepEqual(result[key],f.old[key]);});
for(const [name,failure,published=false,selected=false] of [
 ['Compose resume', 'qz_compose() { echo "compose $*" >> "$LOG"; return 27; }'],
 ['Worker start', 'systemctl() { echo "systemctl $*" >> "$LOG"; return 28; }'],
 ['Worker health', 'systemctl() { echo "systemctl $*" >> "$LOG"; if [[ "$*" == *MainPID* ]]; then printf 0; fi; }'],
 ['API transport', 'curl() { return 29; }'],
 ['API health', 'curl() { printf 503; }'],
 ['frontend health', 'curl() { if [[ "$*" == *health/live ]]; then printf 204; else printf unavailable > "$QZ_WORK/frontend.html"; fi; }'],
 ['installation pre-rename sync', 'sync() { if [[ "${@: -1}" == "$ROOT"/.quazonai-write.* ]]; then return 30; fi; command sync "$@"; }'],
 ['installation rename', 'mv() { if [[ "${@: -1}" == "$ROOT/installation.json" ]]; then return 31; fi; command mv "$@"; }'],
 ['installation post-rename sync', 'sync() { if [[ "${@: -1}" == "$ROOT" && $(qz_get "$ROOT/installation.json" version) == v1.0.1 && $(readlink "$ROOT/current") == "$ROOT/releases/v1.0.0" ]]; then return 32; fi; command sync "$@"; }',true],
 ['current rename', 'mv() { if [[ "${@: -1}" == "$ROOT/current" ]]; then return 33; fi; command mv "$@"; }',true],
 ['current sync', 'sync() { if [[ "${@: -1}" == "$ROOT" && $(readlink "$ROOT/current") == "$ROOT/releases/v1.0.1" ]]; then return 34; fi; command sync "$@"; }',true,true],
 ['Worker enable', 'systemctl() { echo "systemctl $*" >> "$LOG"; if [[ "$*" == *MainPID* ]]; then printf 123; elif [[ "$*" == *enable* ]]; then return 35; fi; }',true,true],
 ['restart policy', 'qz_restarts() { echo "restarts $2" >> "$LOG"; return 36; }',true,true],
 ['pending unlink', 'rm() { if [[ "${@: -1}" == "$ROOT/pending.json" ]]; then return 37; fi; command rm "$@"; }',true,true],
 ['pending removal sync', 'sync() { if [[ "${@: -1}" == "$ROOT" && ! -e "$ROOT/pending.json" ]]; then return 38; fi; command sync "$@"; }',true,true],
])test(`starting ${name} failure preserves recovery evidence and retries without migration`,async t=>{
 const f=await lifecycle(t,'starting'),pending=join(f.old.root,'pending.json'),manifest=join(f.old.root,'installation.json'),original=await readFile(pending,'utf8');
 const result=run(f.root,f.library+failure+'\nqz_apply_update "$ROOT"');
 assert.equal(result.signal,null);assert.equal(typeof result.status,'number');assert.notEqual(result.status,0);assert.doesNotMatch(result.stdout,/Active release|Already active/);
 assert.doesNotMatch(await f.log(),/migrate|configure-worker|compose stop|backup|init-state|unexpected-docker/);
 assert.equal(await readFile(pending,'utf8'),original);
 assert.equal(await readFile(manifest,'utf8'),JSON.stringify(published?f.target:f.old));
 assert.equal(await readlink(join(f.old.root,'current')),join(f.old.root,'releases',selected?f.target.version:f.old.version));
 for(const [file,value] of f.retained)assert.equal(await readFile(file,'utf8'),value);
 await writeFile(join(f.root,'calls'),'');
 const retry=run(f.root,f.library+'qz_apply_update "$ROOT"');assert.match(success(retry),/Active release v1.0.1/);
 assert.deepEqual(JSON.parse(await readFile(manifest,'utf8')),f.target);await assert.rejects(readFile(pending),{code:'ENOENT'});
 const log=await f.log();assert.match(log,/compose up -d --no-recreate/);assert.match(log,/systemctl --user start/);assert.match(log,/processor-version v1.0.1/);assert.doesNotMatch(log,/migrate|configure-worker|compose stop|backup|init-state|processor-version v1.0.0|unexpected-docker/);
 for(const [file,value] of f.retained)assert.equal(await readFile(file,'utf8'),value);
 t.diagnostic(`injected failure exit=${result.status}; same-target retry exit=${retry.status}`);
});
test('migration failure retains the pending marker and never resumes old code',async t=>{const f=await lifecycle(t,'migrating');const result=run(f.root,f.library+`qz_compose() { shift; echo "compose $*" >> "$LOG"; if [[ " $* " == *' migrate '* ]]; then return 27; fi; }\nqz_apply_update "$ROOT"`);assert.notEqual(result.status,0);assert.equal(JSON.parse(await readFile(join(f.old.root,'pending.json'),'utf8')).phase,'migrating');assert.doesNotMatch(await f.log(),/systemctl --user (?:start|enable)|compose up -d|restarts true|backup/);});
test('failed old Worker recovery preserves preparing marker',async t=>{const root=await fixture(t),config=await installation(root),file=join(root,'config.json');await writeFile(file,JSON.stringify(config));await writeFile(join(config.root,'pending.json'),'{"phase":"preparing"}');const body=`qz_compose() { if [[ " $* " == *' stop app '* ]]; then return 17; fi; }\nsystemctl() { return 23; }\nqz_restarts() { :; }\nqz_shutdown_and_backup ${quote(file)} preparing`;assert.notEqual(run(root,body).status,0);assert.equal(JSON.parse(await readFile(join(config.root,'pending.json'),'utf8')).phase,'preparing');});
test('same-version verifies health and downgrade fails without migration',async t=>{const f=await lifecycle(t);await writeFile(join(f.root,'candidate.json'),JSON.stringify(f.old));success(run(f.root,f.library.replaceAll('v1.0.1','v1.0.0')+'qz_apply_update "$ROOT"'));assert.doesNotMatch(await f.log(),/migrate|backup|systemctl --user (?:start|enable)|compose up -d|restarts true/);assert.notEqual(run(f.root,f.library.replaceAll('v1.0.1','v0.9.9')+'qz_apply_update "$ROOT"').status,0);});
test('failed port preflight is explicit',async t=>{const root=await fixture(t),result=run(root,'timeout() { return 1; }\nqz_available_ports 8081 55432');assert.notEqual(result.status,0);assert.match(result.stderr,/ports are unavailable/);});
test('failed pre-rename sync preserves the previous complete JSON file',async t=>{const root=await fixture(t),file=join(root,'state.json');await writeFile(file,'{"old":true}\n');const result=run(root,`sync() { return 23; }\nprintf '{"new":true}\\n' | qz_atomic_text ${quote(file)}`);assert.notEqual(result.status,0);assert.equal(await readFile(file,'utf8'),'{"old":true}\n');});
test('Compose clears inherited overrides and pins the original local daemon',async t=>{const root=await fixture(t),config=await installation(root),file=join(root,'config.json');await writeFile(file,JSON.stringify(config));const result=run(root,`export COMPOSE_FILE=/wrong COMPOSE_PROJECT_NAME=wrong DOCKER_CONTEXT=wrong DOCKER_HOST=unix:///wrong.sock\ndocker() { [[ ! -v COMPOSE_FILE && ! -v COMPOSE_PROJECT_NAME && ! -v DOCKER_CONTEXT && "$DOCKER_HOST" == unix:///var/run/docker.sock ]] || return 77; printf '%s\\n' "$*"; }\nqz_compose ${quote(file)} config --quiet`);const out=success(result);assert.match(out,/--project-name quazonai-fixture --env-file \/dev\/null/);assert.match(out,/config --quiet/);});
test('preparing retry with an active Run stops before shutdown or migration',async t=>{const f=await lifecycle(t,'preparing');const result=run(f.root,f.library+'qz_require_idle() { echo active-run >> "$LOG"; return 19; }\nqz_apply_update "$ROOT"');assert.notEqual(result.status,0);assert.equal(JSON.parse(await readFile(join(f.old.root,'pending.json'),'utf8')).phase,'preparing');const log=await f.log();assert.match(log,/systemctl --user enable --now/);assert.doesNotMatch(log,/compose stop|migrate|backup|restarts false/);});
test('new install rejects occupied ports before persisting identity',async t=>{const root=await fixture(t),dir=join(root,'new-cluster');const result=run(root,`QZ_PORT=8081; QZ_DATABASE_PORT=55432; QZ_CODEX_HOME=\nqz_manifest() { :; }\nqz_preflight() { :; }\ntimeout() { return 1; }\nqz_deploy ${quote(dir)}`);assert.notEqual(result.status,0);assert.match(result.stderr,/ports are unavailable/);await assert.rejects(readFile(join(dir,'installation.json')),{code:'ENOENT'});});
test('deterministic nested Unicode fixtures roundtrip through strict canonical JSON',async t=>{const root=await fixture(t),file=join(root,'roundtrip.json');for(let n=0;n<30;n++){const value={['key '+n]:[n,n/10,null,true,false,'饭饭 🚀 \\ " \n'],nested:{['% [x] '+n]:{keep:'$(never)',array:[{n},{}]}},empty:[]};await writeFile(file,JSON.stringify(value));assert.deepEqual(JSON.parse(success(run(root,`qz_raw ${quote(file)} ''`))),value);}});
test('Docker mapping requires a reported string array and rejects rootless or remapped daemons',async t=>{const root=await fixture(t);for(const [data,allowed] of [['["name=seccomp,profile=builtin","name=apparmor"]',true],['[]',true],['null',false],['[3]',false],['["name=rootless"]',false],['["name=userns"]',false]]){const file=join(root,'security.json');await writeFile(file,data);const result=run(root,`docker() { cat ${quote(file)}; }\nqz_docker_mapping`);assert.equal(result.status===0,allowed,data+' '+result.stderr);}});
