#!/usr/bin/env bash
# A published installer is pinned by the producer; the source bootstrap selects dev.
set -euo pipefail
umask 077

version='@QUAZONAI_VERSION@'
directory="$HOME/.local/share/quazonai"
bin_directory="$HOME/.local/bin"
cli_only=false
deployment_args=()
fail() { printf 'Installation failed: %s\n' "$*" >&2; exit 1; }
while [ "$#" -gt 0 ]; do
  case "$1" in
    --help|-h)
      cat <<'HELP'
Usage: bash install.sh [--cli-only] [--version TAG|latest] [--bin-dir PATH] [--directory PATH]
                       [--port PORT] [--database-port PORT] [--codex-home PATH]
Installs the prebuilt CLI into ~/.local/bin. Linux x86_64 also installs or
updates the Docker stack; macOS installs only the CLI. The source bootstrap
selects the newest complete published dev release; release installers are pinned.
--version latest selects the newest complete dev release explicitly.
--cli-only skips the Linux stack. Existing installations retain their settings.
Requires Bash 4.4+ for the Linux stack, Bash 3.2+ for CLI-only, curl, tar
and standard Unix tools; no Python, Node.js or jq.
No local build, automatic package installation, sudo or file checksum is used.
HELP
      exit 0 ;;
    --cli-only) cli_only=true; shift ;;
    --version|--directory|--bin-dir|--port|--database-port|--codex-home)
      [ "$#" -ge 2 ] && [ -n "$2" ] || fail "Missing value for $1"
      case "$1" in
        --version) version=$2 ;;
        --directory) directory=$2 ;;
        --bin-dir) bin_directory=$2 ;;
        *) deployment_args+=("$1" "$2") ;;
      esac
      shift 2 ;;
    *) fail "Unknown option: $1" ;;
  esac
done
expand_home() {
  case "$1" in '~') printf '%s' "$HOME" ;; '~/'*) printf '%s/%s' "$HOME" "${1:2}" ;; *) printf '%s' "$1" ;; esac
}
directory=$(expand_home "$directory")
bin_directory=$(expand_home "$bin_directory")
valid_version() {
  [[ ${#1} -le 128 && "$1" =~ ^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-[0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*)?$ ]] || return 1
  if [[ "$1" == *-* ]]; then
    local identifier
    local identifiers=()
    IFS=. read -r -a identifiers <<< "${1#*-}"
    for identifier in "${identifiers[@]}"; do
      [[ ! "$identifier" =~ ^0[0-9]+$ ]] || return 1
    done
  fi
}
case "$(uname -s)/$(uname -m)" in
  Linux/x86_64) platform=linux-x86_64 ;;
  Darwin/x86_64) platform=macos-x86_64; cli_only=true ;;
  Darwin/arm64) platform=macos-aarch64; cli_only=true ;;
  *) fail 'Supported hosts: Linux x86_64, macOS x86_64 or Apple Silicon. Use install.ps1 on Windows x86_64.' ;;
esac
if ! "$cli_only"; then
  [ "${BASH_VERSINFO[0]}" -gt 4 ] || { [ "${BASH_VERSINFO[0]}" -eq 4 ] && [ "${BASH_VERSINFO[1]}" -ge 4 ]; } || fail 'The Linux Docker stack requires Bash 4.4 or newer. Upgrade Bash explicitly or use --cli-only.'
fi
for tool in curl tar awk sort cmp mktemp; do
  command -v "$tool" >/dev/null || fail "$tool is required."
done
work=$(mktemp -d)
staged=''
trap 'rm -rf -- "$work"; if [ -n "$staged" ]; then rm -f -- "$staged"; fi' EXIT

# This standalone parser must be available before the versioned bundle exists.
# It parses the whole JSON document, rejects duplicate keys and never evaluates data.
json_read() {
  # BSD awk strings cannot retain NUL: sprintf("%c",0) is empty there.
  # Check file bytes in Bash before awk can silently discard them.
  local prefix
  if IFS= read -r -d '' prefix < "$2"; then return 1; fi
  LC_ALL=C awk -v mode="$1" -v field="${3:-}" '
  function bad() { failed=1; exit 2 }
  function ws() { while (substr(s,p,1) ~ /^[ \t\r\n]$/) p++ }
  function hex4(    h,n,i,c) {
    h=substr(s,p,4); if(length(h)!=4 || h ~ /[^0-9a-fA-F]/) bad()
    n=0; for(i=1;i<=4;i++){ c=tolower(substr(h,i,1)); n=n*16+index("0123456789abcdef",c)-1 }
    p+=4; return n
  }
  function utf8(n) {
    if(n==0) bad() # A decoded NUL must not disappear from keys or shell values.
    if(n<128) return sprintf("%c",n)
    if(n<2048) return sprintf("%c%c",192+int(n/64),128+n%64)
    if(n<65536) return sprintf("%c%c%c",224+int(n/4096),128+int(n/64)%64,128+n%64)
    return sprintf("%c%c%c%c",240+int(n/262144),128+int(n/4096)%64,128+int(n/64)%64,128+n%64)
  }
  function string(    out,c,e,n,low,b,length_,i,x) {
    if(substr(s,p++,1)!="\"") bad()
    out=""
    while(p<=length(s)) {
      c=substr(s,p++,1); if(c=="\"") return out
      if(c ~ /^[\001-\037]$/) bad()
      if(c!="\\") {
        n=ordinal[c]
        if(n<128) { out=out c; continue }
        if(n>=194 && n<=223) { length_=1; b=n-192 }
        else if(n>=224 && n<=239) { length_=2; b=n-224 }
        else if(n>=240 && n<=244) { length_=3; b=n-240 }
        else bad()
        for(i=1;i<=length_;i++) {
          x=substr(s,p++,1); n=ordinal[x]
          if(n<128 || n>191 || x=="") bad()
          c=c x; b=b*64+n-128
        }
        if((length_==1 && b<128) || (length_==2 && b<2048) ||
           (length_==3 && b<65536) || b>1114111 || (b>=55296 && b<=57343)) bad()
        out=out c; continue
      }
      e=substr(s,p++,1)
      if(e=="\"" || e=="\\" || e=="/") out=out e
      else if(e=="b") out=out sprintf("%c",8)
      else if(e=="f") out=out sprintf("%c",12)
      else if(e=="n") out=out "\n"
      else if(e=="r") out=out "\r"
      else if(e=="t") out=out "\t"
      else if(e=="u") {
        n=hex4()
        if(n>=55296 && n<=56319) {
          if(substr(s,p,2)!="\\u") bad(); p+=2; low=hex4()
          if(low<56320 || low>57343) bad(); n=65536+(n-55296)*1024+low-56320
        } else if(n>=56320 && n<=57343) bad()
        out=out utf8(n)
      } else bad()
    }
    bad()
  }
  function parse(    id,c,key,child,token) {
    ws(); id=++nodes; c=substr(s,p,1)
    if(c=="{" || c=="[") {
      kind[id]=(c=="{"?"object":"array"); p++; ws()
      if(substr(s,p,1)==(c=="{"?"}":"]")){p++; return id}
      while(1) {
        if(c=="{") { ws(); key=string(); if((id SUBSEP key) in member) bad(); ws(); if(substr(s,p++,1)!=":") bad() }
        else key=count[id]+1
        child=parse(); member[id SUBSEP key]=child; count[id]++
        ws(); token=substr(s,p++,1)
        if(token==(c=="{"?"}":"]")) break
        if(token!=",") bad()
      }
    } else if(c=="\"") { kind[id]="string"; value[id]=string() }
    else {
      token=substr(s,p)
      if(match(token,/^-?(0|[1-9][0-9]*)(\.[0-9]+)?([eE][+-]?[0-9]+)?/)) {
        value[id]=substr(token,1,RLENGTH); kind[id]="number"; p+=RLENGTH
      } else if(substr(token,1,4)=="true" || substr(token,1,4)=="null") {
        value[id]=substr(token,1,4); kind[id]=value[id]; p+=4
      } else if(substr(token,1,5)=="false") { value[id]="false"; kind[id]="false"; p+=5 }
      else bad()
    }
    return id
  }
  function prop(id,key) { return member[id SUBSEP key] }
  function numeric_newer(a,b,    aa,bb,i,na,nb) {
    na=split(a,aa,/[^0-9]+/); nb=split(b,bb,/[^0-9]+/)
    for(i=1;i<=na;i++) {
      if(length(aa[i])!=length(bb[i])) return length(aa[i])>length(bb[i])
      if("x" aa[i]!="x" bb[i]) return "x" aa[i]>"x" bb[i]
    }
    return 0
  }
  function dev(tag,    parts,n,i) {
    if(length(tag)>128 || tag !~ /^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)-dev\.[0-9]+\.[1-9][0-9]*$/) return 0
    n=split(tag,parts,"."); return length(parts[n-1])==14 && substr(parts[n-1],1,1)!="0"
  }
  BEGIN { for(i=0;i<256;i++) ordinal[sprintf("%c",i)]=i }
  { s=s $0 "\n" }
  END {
    if(failed) exit 2
    p=1; root=parse(); ws(); if(p<=length(s)) bad()
    if(mode=="field") {
      if(kind[root]!="object") bad(); node=prop(root,field)
      if(kind[node]!="string" || value[node] ~ /[[:cntrl:]]/) bad(); printf "%s",value[node]; exit
    }
    if(mode!="latest" || kind[root]!="array") bad()
    latest=field
    split("install.sh install.ps1 release.json quazonai-deploy.tar.gz README.md quazonai-cli-linux-x86_64.tar.gz quazonai-cli-macos-x86_64.tar.gz quazonai-cli-macos-aarch64.tar.gz quazonai-cli-windows-x86_64.zip quazonai-image-application.tar.gz quazonai-image-runtime.tar.gz quazonai-image-codex.tar.gz quazonai-image-database.tar.gz",required," ")
    for(i=1;i<=count[root];i++) {
      release=prop(root,i); tag=value[prop(release,"tag_name")]; assets=prop(release,"assets")
      if(kind[release]!="object" || kind[prop(release,"tag_name")]!="string" || !dev(tag) || kind[prop(release,"draft")]!="false" || kind[assets]!="array") continue
      for(name in found) delete found[name]
      duplicate=0
      for(j=1;j<=count[assets];j++) {
        asset=prop(assets,j); name=value[prop(asset,"name")]
        if(kind[asset]=="object" && kind[prop(asset,"name")] == "string" && value[prop(asset,"state")]=="uploaded" && kind[prop(asset,"size")] == "number" && value[prop(asset,"size")]+0>0) {
          if(name in found) duplicate=1
          found[name]=1
        }
      }
      complete=!duplicate; for(k in required) if(!(required[k] in found)) complete=0
      if(complete && (latest=="" || numeric_newer(tag,latest))) latest=tag
    }
    printf "%s %s",count[root]+0,latest
  }' "$2"
}
fetch() {
  curl --fail --location --proto '=https' --proto-redir '=https' --tlsv1.2 \
    --retry 3 --connect-timeout 15 --max-time 600 --output "$2" "$1"
}
unresolved='@QUAZONAI_''VERSION@'
if [ "$version" = "$unresolved" ] || [ "$version" = latest ]; then
  page=1
  selected=''
  while :; do
    endpoint='https://api.github.com/repos/zhengui666/QuaZonai/releases?per_page=100'
    [ "$page" -eq 1 ] || endpoint="$endpoint&page=$page"
    fetch "$endpoint" "$work/releases.json" || fail 'Could not download the release catalog.'
    selection=$(json_read latest "$work/releases.json" "$selected") || fail 'Invalid release catalog; no installation was changed.'
    IFS=' ' read -r count selected <<< "$selection"
    [ "$count" -eq 100 ] || break
    page=$((page + 1))
  done
  [ -n "$selected" ] || fail 'No complete published dev release was found. Supply --version TAG for a specific release.'
  version=$selected
fi
valid_version "$version" || fail 'Expected a published vMAJOR.MINOR.PATCH[-prerelease] tag.'
base="https://github.com/zhengui666/QuaZonai/releases/download/$version"

extract_files() {
  local archive=$1 destination=$2 name
  shift 2
  tar -tzf "$archive" > "$work/members" || fail 'Invalid or incomplete archive.'
  LC_ALL=C sort "$work/members" > "$work/actual"
  printf '%s\n' "$@" | LC_ALL=C sort > "$work/expected"
  cmp -s "$work/actual" "$work/expected" || fail 'Unexpected or duplicate archive files.'
  tar -tvzf "$archive" > "$work/headers" || fail 'Invalid archive headers.'
  LC_ALL=C awk 'substr($0,1,1)!="-" {exit 1}' "$work/headers" || fail 'Archive members must be regular files, not links or directories.'
  mkdir "$destination"
  # Write only explicitly named contents into a new private directory. Never
  # materialize archive paths, links, permissions or ownership on the host.
  for name in "$@"; do
    tar -xOzf "$archive" "$name" > "$destination/$name" || fail "Could not extract $name"
    [ -s "$destination/$name" ] || fail "Empty archive member: $name"
  done
}
archive="quazonai-cli-$platform.tar.gz"
fetch "$base/$archive" "$work/$archive" || fail 'Could not download the CLI archive.'
extract_files "$work/$archive" "$work/cli" quazonai LICENSE NOTICE THIRD_PARTY_NOTICES.md
chmod 755 "$work/cli/quazonai"
actual_version=$("$work/cli/quazonai" --version) || fail 'The downloaded CLI cannot run on this host.'
[ "$actual_version" = "quazonai $version" ] || fail 'Downloaded CLI does not match the requested tag.'
mkdir -p -- "$bin_directory"
bin_directory=$(cd -- "$bin_directory" && pwd -P)
[ ! -d "$bin_directory/quazonai" ] || fail 'The CLI destination is a directory.'
staged=$(mktemp "$bin_directory/.quazonai.XXXXXX")
cp "$work/cli/quazonai" "$staged"
chmod 755 "$staged"
if ! "$cli_only"; then
  fetch "$base/quazonai-deploy.tar.gz" "$work/quazonai-deploy.tar.gz" || fail 'Could not download the deployment bundle.'
  extract_files "$work/quazonai-deploy.tar.gz" "$work/deployment" \
    manage.sh json.awk codex.sh deploy.sh update.sh compose.yaml release.json README.md \
    codex-update.sh codex-login.sh runtime.sh codex.apparmor .env.example
  release_version=$(json_read field "$work/deployment/release.json" version) || fail 'Invalid deployment manifest.'
  [ "$release_version" = "$version" ] || fail 'Downloaded deployment bundle does not match the requested tag.'
  pending=''
  if [ -e "$directory/pending.json" ] || [ -L "$directory/pending.json" ]; then
    pending=$(json_read field "$directory/pending.json" operation) || fail 'Invalid pending installation record; preserve it for recovery.'
    [ -f "$directory/installation.json" ] && { [ "$pending" = install ] || [ "$pending" = update ]; } || fail 'Preserve and reconcile the interrupted installation before retrying.'
  fi
  command=apply-update
  if [ ! -f "$directory/installation.json" ] || [ "$pending" = install ] || { [ -z "$pending" ] && [ ! -L "$directory/current" ]; }; then
    command=deploy
  fi
  # Use the NEW manager even when the old installation used Python. The old
  # update.sh cannot unpack this new bundle file set. No existing state is copied.
  bash "$work/deployment/manage.sh" "$command" --directory "$directory" ${deployment_args[@]+"${deployment_args[@]}"}
fi
licenses="$HOME/.local/share/quazonai-cli/licenses/$version"
mkdir -p -- "$licenses"
cp "$work/cli/LICENSE" "$work/cli/NOTICE" "$work/cli/THIRD_PARTY_NOTICES.md" "$licenses/"
# Both paths are on the same filesystem; a failed download/deploy leaves the old CLI.
mv -f -- "$staged" "$bin_directory/quazonai"
staged=''
printf 'Installed QuaZonai CLI from %s: %s/quazonai\n' "$version" "$bin_directory"
case ":$PATH:" in
  *":$bin_directory:"*) ;;
  *) printf 'Add it to your shell PATH: export PATH=%q:"$PATH"\n' "$bin_directory" ;;
esac
