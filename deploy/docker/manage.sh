#!/usr/bin/env bash
# Versioned, prebuilt-image deployment. Bash + standard Linux utilities; no host Python.
set -euo pipefail
umask 077
QZ_BUNDLE=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
QZ_DATABASE_IMAGE='ghcr.io/pgmq/pg18-pgmq@sha256:bfb3537068ce453609744518ece92b178ac89dff53747d47ca6fab91c2fc66a6'
QZ_BUNDLE_FILES=(manage.sh json.awk deploy.sh update.sh compose.yaml release.json README.md codex.sh codex-update.sh codex-login.sh runtime.sh codex.apparmor .env.example)
qz_fail() { printf 'Deployment failed: %s\n' "$*" >&2; exit 1; }
# Help also works in macOS's stock Bash; real Linux management checks below.
shopt -s inherit_errexit 2>/dev/null || :
qz_require_bash() { shopt -s inherit_errexit 2>/dev/null || qz_fail 'The Linux deployment manager requires Bash 4.4 or newer.'; }
qz_expand_path() {
    case "$1" in '~') printf '%s' "$HOME";; '~/'*) printf '%s/%s' "$HOME" "${1:2}";; *) printf '%s' "$1";; esac
}
qz_json() { LC_ALL=C awk -v mode="$1" -v path="$2" -f "$QZ_BUNDLE/json.awk" "${@:3}"; }
qz_get() {
    local result
    # Reject NUL in the parser and retain trailing newlines until validation.
    result=$(qz_json get "$2" "$1" && printf '\001') || qz_fail "Cannot read deployment field: $2"
    result=${result%$'\001'}
    [[ "$result" != *[$'\001'-$'\037'$'\177']* ]] || qz_fail "Control characters cannot enter deployment scalar fields: $2"
    printf '%s' "$result"
}
qz_raw() { qz_json raw "$2" "$1"; }
qz_type() { qz_json type "$2" "$1"; }
qz_quote() { printf '%s\001' "$1" | qz_json quote ''; }
qz_has() { qz_json type "$2" "$1" >/dev/null 2>&1; }
qz_directory() { mkdir -p -- "$1"; sync -f -- "$1"; sync -f -- "$(dirname -- "$1")"; }
qz_atomic_text() {
    local temporary parent
    parent=$(dirname -- "$1"); temporary=$(mktemp "$parent/.quazonai-write.XXXXXX")
    # Pre-rename failure preserves the original. A failed post-rename sync is
    # reported as uncertain durability; callers retain their recovery marker.
    if ! cat > "$temporary"; then rm -f -- "$temporary"; qz_fail 'Could not write temporary deployment state.'; fi
    if [[ "$1" == *.json ]] && ! qz_json canonical '' "$temporary" >/dev/null; then
        rm -f -- "$temporary"; qz_fail 'Invalid JSON was not written over deployment state.'
    fi
    if ! (chmod 600 "$temporary" && sync -f -- "$temporary" && mv -fT -- "$temporary" "$1" && sync -f -- "$parent"); then
        rm -f -- "$temporary"; qz_fail 'Could not durably replace deployment state.'
    fi
}
qz_save() { qz_atomic_text "$1" < "$2"; }
qz_merge() {
    local staged
    staged=$(mktemp "${QZ_WORK:-${TMPDIR:-/tmp}}/quazonai-merge.XXXXXX")
    if ! qz_json merge '' "$1" "$2" > "$staged"; then rm -f -- "$staged"; qz_fail 'Invalid JSON merge left the original state unchanged.'; fi
    qz_save "$3" "$staged"
    rm -f -- "$staged"
}
qz_lock() {
    qz_directory "$1"
    exec 9>>"$1/.deployment.lock"
    flock -n 9 || qz_fail 'Another deployment or Codex operation holds the installation lock.'
}
qz_version() {
    local identifier
    [[ ${#1} -le 128 && "$1" =~ ^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-[0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*)?$ ]] || qz_fail 'Expected vMAJOR.MINOR.PATCH[-prerelease], without build metadata.'
    if [[ "$1" == *-* ]]; then
        local parts; IFS=. read -r -a parts <<< "${1#*-}"
        for identifier in "${parts[@]}"; do [[ ! "$identifier" =~ ^0[0-9]+$ ]] || qz_fail 'Numeric prerelease identifiers cannot have leading zeroes.'; done
    fi
}
qz_precedence() {
    # Compare unbounded decimal identifiers as strings, never awk floating numbers.
    LC_ALL=C awk -v a="$1" -v b="$2" '
    function decimal(a,b) { if(length(a)!=length(b)) return length(a)<length(b)?-1:1; return ("x" a)==("x" b)?0:(("x" a)<("x" b)?-1:1) }
    BEGIN { sub(/^v/,"",a); sub(/^v/,"",b); split(a,x,"-"); split(b,y,"-"); split(x[1],ac,"."); split(y[1],bc,".");
      for(i=1;i<=3;i++) { c=decimal(ac[i],bc[i]); if(c) { print c; exit } }
      ap=substr(a,length(x[1])+2); bp=substr(b,length(y[1])+2);
      if(ap=="" || bp=="") { print ap==bp?0:(ap==""?1:-1); exit }
      na=split(ap,aa,"."); nb=split(bp,bb,".");
      for(i=1;i<=na && i<=nb;i++) { an=aa[i]~/^[0-9]+$/; bn=bb[i]~/^[0-9]+$/;
        if(an && bn) c=decimal(aa[i],bb[i]); else if(an!=bn) c=an?-1:1; else c=("x" aa[i])==("x" bb[i])?0:(("x" aa[i])<("x" bb[i])?-1:1);
        if(c) { print c; exit }
      } print na==nb?0:(na<nb?-1:1)
    }'
}
qz_manifest() {
    local file=$1 field repository reference
    [[ $(qz_type "$file" '') == object && $(qz_type "$file" schema_version) == number && $(qz_get "$file" schema_version) == 2 ]] || qz_fail 'A version-2 prebuilt-image deployment manifest is required.'
    qz_version "$(qz_get "$file" version)"
    [[ $(qz_get "$file" revision) =~ ^[0-9a-f]{40}$ ]] || qz_fail 'Release revision is invalid.'
    for field in image runtime_image codex_image; do
        case "$field" in image) repository=quazonai;; runtime_image) repository=quazonai-runtime;; codex_image) repository=quazonai-codex;; esac
        reference=$(qz_get "$file" "$field")
        [[ "$reference" =~ ^ghcr\.io/zhengui666/$repository@sha256:[0-9a-f]{64}$ || "$reference" =~ ^sha256:[0-9a-f]{64}$ ]] || qz_fail "$field must reference a prebuilt image by digest."
    done
    qz_version "v$(qz_get "$file" codex_version)"
    [[ $(qz_get "$file" database_image) == "$QZ_DATABASE_IMAGE" ]] || qz_fail 'Unsupported PostgreSQL/PGMQ image.'
}
qz_configuration() {
    QZ_CONFIG="$1/installation.json"
    [[ -f "$QZ_CONFIG" && ! -L "$QZ_CONFIG" ]] || qz_fail 'Restore this installation’s original manifest before continuing.'
    [[ $(qz_type "$QZ_CONFIG" '') == object && $(qz_get "$QZ_CONFIG" uid) == "$(id -u)" && $(qz_get "$QZ_CONFIG" root) == "$1" ]] || qz_fail 'Manage this installation as its original user at its original path.'
    qz_validate_configuration "$QZ_CONFIG"
}
qz_validate_configuration() {
    local field
    for field in root home codex_home unit_directory path password project bundle version revision image database_image; do
        [[ $(qz_type "$1" "$field") == string ]] || qz_fail "Invalid string field: $field"
        qz_get "$1" "$field" >/dev/null || return $?
    done
    for field in uid gid port database_port; do
        [[ $(qz_type "$1" "$field") == number && $(qz_get "$1" "$field") =~ ^(0|[1-9][0-9]*)$ ]] || qz_fail "Invalid integer field: $field"
    done
    qz_ports "$(qz_get "$1" port)" "$(qz_get "$1" database_port)"
    qz_version "$(qz_get "$1" version)"
    qz_validate_paths "$1"
}
qz_preflight() {
    local binary endpoint architecture options version major minor
    [[ $(uname -s)/$(uname -m) == Linux/x86_64 && $(id -u) != 0 ]] || qz_fail 'Run as the non-root Linux x86_64 installation owner, not with sudo.'
    [[ -f /sys/fs/cgroup/cgroup.controllers ]] || qz_fail 'Native Missions require cgroup v2.'
    for binary in docker systemctl systemd-analyze systemd-socket-activate loginctl git curl tar awk flock realpath stat sync sha256sum od timeout; do command -v "$binary" >/dev/null || qz_fail "Required executable is missing: $binary"; done
    endpoint=${DOCKER_HOST:-$(docker context inspect --format '{{.Endpoints.docker.Host}}')}
    [[ "$endpoint" == unix://* ]] || qz_fail 'Use a local Unix-socket Docker daemon.'
    architecture=$(docker info --format '{{.OSType}}/{{.Architecture}}')
    [[ "$architecture" == linux/x86_64 || "$architecture" == linux/amd64 ]] || qz_fail 'This native release supports Linux x86_64.'
    qz_docker_mapping
    version=$(docker compose version --short)
    [[ "$version" =~ ^v?([0-9]+)\.([0-9]+) ]] || qz_fail 'Docker Compose 2.20 or newer is required.'
    major=${BASH_REMATCH[1]}; minor=${BASH_REMATCH[2]}
    ((10#$major > 2 || (10#$major == 2 && 10#$minor >= 20))) || qz_fail 'Docker Compose 2.20 or newer is required.'
    systemctl --user show-environment >/dev/null
    [[ $(loginctl show-user "$(id -u)" --property=Linger --value) == yes ]] || qz_fail 'Enable persistent user services first: loginctl enable-linger "$USER"'
}
qz_docker_mapping() {
    local file="$QZ_WORK/docker-security.json" n i option part
    local parts=()
    docker info --format '{{json .SecurityOptions}}' > "$file"
    [[ $(qz_type "$file" '') == array ]] || qz_fail 'Docker did not report its user namespace configuration.'
    n=$(qz_json length '' "$file")
    for ((i=0;i<n;i++)); do
        [[ $(qz_type "$file" "$i") == string ]] || qz_fail 'Docker security options must be strings.'
        option=$(qz_get "$file" "$i"); IFS=, read -r -a parts <<< "$option"
        for part in "${parts[@]}"; do
            part=${part#name=}
            [[ "$part" != rootless && "$part" != userns ]] || qz_fail 'Use a rootful Docker daemon without userns-remap.'
        done
    done
}
qz_validate_paths() {
    local file=$1 name value root codex_home
    for name in root home codex_home unit_directory; do value=$(qz_get "$file" "$name"); [[ "$value" == /* ]] || qz_fail "$name must be absolute."; done
    root=$(qz_get "$file" root)
    [[ "$root" != *[:\"\\]* ]] || qz_fail 'Installation path cannot contain colon, double quote or backslash.'
    root=$(realpath -m -- "$root"); codex_home=$(realpath -m -- "$(qz_get "$file" codex_home)")
    [[ "$codex_home" != "$root" && "$codex_home" != "$root/"* && "$root" != "$codex_home/"* ]] || qz_fail 'CODEX_HOME must be separate from the installation, including through symlinks.'
    qz_get "$file" path >/dev/null
}
qz_ports() {
    [[ "$1" =~ ^[0-9]{1,5}$ && "$2" =~ ^[0-9]{1,5}$ ]] || qz_fail 'Select numeric web/database ports.'
    ((10#$1 >= 1024 && 10#$1 <= 65535 && 10#$2 >= 1024 && 10#$2 <= 65535 && 10#$1 != 10#$2)) || qz_fail 'Select distinct ports between 1024 and 65535.'
}
qz_available_ports() {
    local status=0
    # Existing systemd's socket activator binds both sockets before spawning
    # anything. Timeout means both binds stayed available for this observation.
    timeout 1 systemd-socket-activate --listen="127.0.0.1:$1" --listen="127.0.0.1:$2" /bin/true > /dev/null 2> "$QZ_WORK/ports.log" || status=$?
    [[ "$status" == 0 || "$status" == 124 ]] || qz_fail 'Web/database ports are unavailable; select other ports before installing.'
}
qz_compose() (
    local file=$1 name root bundle project
    shift
    for name in ${!COMPOSE_@}; do unset "$name"; done
    root=$(qz_get "$file" root); bundle=$(qz_get "$file" bundle); project=$(qz_get "$file" project)
    APP_IMAGE=$(qz_get "$file" image); DATABASE_IMAGE=$(qz_get "$file" database_image)
    DATABASE_PASSWORD=$(qz_get "$file" password); INSTALL_DIR="$root"
    WEB_PORT=$(qz_get "$file" port); DATABASE_PORT=$(qz_get "$file" database_port)
    HOST_UID=$(qz_get "$file" uid); HOST_GID=$(qz_get "$file" gid)
    NATIVE_CODEX_HOME=$(qz_get "$file" codex_home); CODEX_IMAGE="quazonai-codex:$project"
    export APP_IMAGE DATABASE_IMAGE DATABASE_PASSWORD INSTALL_DIR WEB_PORT DATABASE_PORT HOST_UID HOST_GID NATIVE_CODEX_HOME CODEX_IMAGE
    export CODEX_DOCKER_SOCKET=/var/run/docker.sock DOCKER_SOCKET_GID="$HOST_GID"
    if qz_has "$file" codex_runtime_image; then CODEX_IMAGE=$(qz_get "$file" codex_runtime_image); fi
    if qz_has "$file" docker_socket; then CODEX_DOCKER_SOCKET=$(qz_get "$file" docker_socket); fi
    if qz_has "$file" docker_socket_gid; then DOCKER_SOCKET_GID=$(qz_get "$file" docker_socket_gid); fi
    # Maintenance stays bound to the original daemon, even if the operator has
    # selected another ambient context after installation.
    if qz_has "$file" docker_socket; then export DOCKER_HOST="unix://$CODEX_DOCKER_SOCKET"; unset DOCKER_CONTEXT; fi
    export APP_RESTART_POLICY=no RUNTIME_TARGETS='[]' DOWNSTREAM_TARGETS='[]'
    if [[ -L "$root/current" && ! -e "$root/pending.json" ]]; then APP_RESTART_POLICY=unless-stopped; fi
    if qz_has "$file" runtime_targets; then RUNTIME_TARGETS=$(qz_raw "$file" runtime_targets); fi
    if qz_has "$file" downstream_targets; then DOWNSTREAM_TARGETS=$(qz_raw "$file" downstream_targets); fi
    local command=(docker compose --project-name "$project" --env-file /dev/null -f "$bundle/compose.yaml")
    if [[ -f "$root/compose.override.yaml" ]]; then command+=(-f "$root/compose.override.yaml"); fi
    "${command[@]}" "$@"
)
qz_unit() { printf '%s.service' "$(qz_get "$1" project)"; }
qz_sql() { qz_compose "$1" exec -T database psql -X -U quazonai -d quazonai -v ON_ERROR_STOP=1 -Atc "$2"; }
qz_require_idle() { [[ $(qz_sql "$1" 'SELECT count(*) FROM app.runs WHERE finished_at IS NULL') == 0 ]] || qz_fail 'Unfinished Runs exist. Finish or cancel/reconcile them; no task records were changed.'; }
qz_restarts() {
    local file=$1 enabled=$2 ids id policy=no; local containers=()
    ids=$(qz_compose "$file" ps --all --quiet app) || return $?
    if [[ -n "$ids" ]]; then while IFS= read -r id; do containers+=("$id"); done <<< "$ids"; fi
    if [[ "$enabled" == true ]]; then [[ ${#containers[@]} == 1 ]] || qz_fail 'Activation requires exactly one application container.'; policy=unless-stopped; fi
    if ((${#containers[@]})); then
        (
            if qz_has "$file" docker_socket; then DOCKER_HOST="unix://$(qz_get "$file" docker_socket)"; export DOCKER_HOST; unset DOCKER_CONTEXT; fi
            docker update "--restart=$policy" "${containers[@]}" >/dev/null
        ) || return $?
    fi
    return 0
}
qz_native_binaries() {
    [[ "$1/quazonai" -ef "$1/server" ]] || qz_fail 'The CLI must use the packaged server executable.'
    "$1/server" --version
    "$1/runtime" --version
    "$1/quazonai" client --help >/dev/null
}
qz_systemd_quote() {
    local value=$1
    value=${value//\\/\\\\}; value=${value//\"/\\\"}
    if [[ ${2:-true} == true ]]; then value=${value//%/%%}; fi
    printf '"%s"' "$value"
}
qz_systemd_path() {
    local value=${1//%/%%}
    if [[ ${2:-false} == true ]]; then value=${value//\\/\\\\}; value=${value//\*/\\*}; value=${value//\?/\\?}; value=${value//\[/\\[}; value=${value//\]/\\]}; fi
    printf '%s' "$value"
}
qz_worker_unit() {
    local root version
    root=$(qz_get "$1" root); version=$(qz_get "$1" version)
    printf '[Unit]\nDescription=QuaZonai native Worker for the container deployment\n[Service]\nType=simple\nUMask=0077\nWorkingDirectory=%s\nEnvironmentFile=%s\nExecStart=:%s worker\nRestart=on-failure\nRestartSec=3\nTimeoutStopSec=90\n[Install]\nWantedBy=default.target\n' \
      "$(qz_systemd_path "$root/data")" "$(qz_systemd_path "$root/worker.env" true)" "$(qz_systemd_quote "$root/releases/$version/bin/server")"
}
qz_worker_environment() {
    local file=$1 root version name value project password database_port
    qz_validate_configuration "$file"
    root=$(qz_get "$file" root); version=$(qz_get "$file" version); project=$(qz_get "$file" project)
    password=$(qz_get "$file" password); database_port=$(qz_get "$file" database_port)
    while IFS= read -r name; do
        case "$name" in
          DATABASE_URL) value="postgresql://quazonai:$password@127.0.0.1:$database_port/quazonai";;
          STATE_DIR) value="$root/data/state";; HOME) value=$(qz_get "$file" home);; CODEX_HOME) value=$(qz_get "$file" codex_home);;
          PUBLIC_URL) value="http://localhost:$(qz_get "$file" port)";; CODEX_IMAGE) value="quazonai-codex:$project"; if qz_has "$file" codex_runtime_image; then value=$(qz_get "$file" codex_runtime_image); fi;;
          CODEX_DOCKER_SOCKET) value=/var/run/docker.sock; if qz_has "$file" docker_socket; then value=$(qz_get "$file" docker_socket); fi;;
          CODEX_LOCK_FILE) value="$root/.deployment.lock";; DEVELOPMENT_HTTP) value=true;; PATH) value="$root/releases/$version/bin:$(qz_get "$file" path)";;
          RUNTIME_TARGETS) value='[]'; if qz_has "$file" runtime_targets; then value=$(qz_raw "$file" runtime_targets); fi;;
          DOWNSTREAM_TARGETS) value='[]'; if qz_has "$file" downstream_targets; then value=$(qz_raw "$file" downstream_targets); fi;;
        esac
        printf '%s=%s\n' "$name" "$(qz_systemd_quote "$value" false)"
    done <<'FIELDS'
DATABASE_URL
STATE_DIR
HOME
CODEX_HOME
PUBLIC_URL
CODEX_IMAGE
CODEX_DOCKER_SOCKET
CODEX_LOCK_FILE
DEVELOPMENT_HTTP
PATH
RUNTIME_TARGETS
DOWNSTREAM_TARGETS
FIELDS
}
qz_configure_worker() {
    local root units
    root=$(qz_get "$1" root); units=$(qz_get "$1" unit_directory)
    qz_directory "$units"
    qz_worker_environment "$1" > "$QZ_WORK/worker.env"
    qz_worker_unit "$1" > "$QZ_WORK/worker.service"
    qz_save "$root/worker.env" "$QZ_WORK/worker.env"
    qz_save "$units/$(qz_unit "$1")" "$QZ_WORK/worker.service"
    systemctl --user daemon-reload
}
qz_verify_worker() {
    local before after unit
    unit=$(qz_unit "$1"); before=$(systemctl --user show "$unit" --property=MainPID --value)
    [[ "$before" =~ ^[1-9][0-9]*$ ]] || qz_fail 'Native Worker did not start.'
    sleep 3
    systemctl --user is-active --quiet "$unit"
    after=$(systemctl --user show "$unit" --property=MainPID --value)
    [[ "$before" == "$after" ]] || qz_fail 'Native Worker restarted during startup; inspect its journal.'
}
qz_verify_console() {
    local origin status
    origin="http://localhost:$(qz_get "$1" port)"
    status=$(curl --silent --show-error --noproxy '*' --max-time 10 --output /dev/null --write-out '%{http_code}' "$origin/health/live")
    [[ "$status" == 204 ]] || qz_fail 'API liveness check failed.'
    curl --fail --silent --show-error --noproxy '*' --max-time 10 --header 'Accept: text/html' --output "$QZ_WORK/frontend.html" "$origin/"
    head -c 4096 "$QZ_WORK/frontend.html" | LC_ALL=C grep -qi '<!doctype html>' || qz_fail 'Production frontend was not served.'
}
qz_resume() {
    local file=$1 enable=${2:-true} status=0
    # Starting-phase recovery does not recreate or restart running processors.
    qz_compose "$file" up -d --no-recreate --wait --wait-timeout 120 app || status=$?
    if [[ "$enable" == true ]]; then systemctl --user enable --now "$(qz_unit "$file")" || return $?; else systemctl --user start "$(qz_unit "$file")" || return $?; fi
    return "$status"
}
qz_activate() {
    local root=$1 file=$2 version
    qz_verify_console "$file"
    qz_save "$root/installation.json" "$file"
    version=$(qz_get "$file" version)
    rm -f -- "$root/current.next"
    ln -s -- "$root/releases/$version" "$root/current.next"
    mv -fT -- "$root/current.next" "$root/current"
    sync -f -- "$root"
    systemctl --user enable "$(qz_unit "$file")"
    qz_restarts "$file" true
    # Keep same-target recovery available if clearing the marker cannot be synced.
    qz_save "$QZ_WORK/completed-pending.json" "$root/pending.json"
    if ! (rm -f -- "$root/pending.json" && sync -f -- "$root"); then
        qz_save "$root/pending.json" "$QZ_WORK/completed-pending.json"
        qz_fail 'Could not durably clear the recovery marker; retry the same target.'
    fi
    printf 'Active release %s: http://localhost:%s\n' "$version" "$(qz_get "$file" port)" || :
}
qz_start_prepared() { qz_resume "$2" false; qz_verify_worker "$2"; qz_activate "$1" "$2"; }
qz_prepare() {
    local bundle=$1 file=$2 release="$1/release.json" candidate root endpoint socket project destination stored field image revision name container
    qz_validate_paths "$file"; qz_manifest "$release"
    root=$(qz_get "$file" root); project=$(qz_get "$file" project)
    endpoint=${DOCKER_HOST:-$(docker context inspect --format '{{.Endpoints.docker.Host}}')}
    [[ "$endpoint" == unix://* ]] || qz_fail 'Codex containers require a local Docker Unix socket.'
    socket=$(realpath -e -- "${endpoint#unix://}")
    [[ -S "$socket" ]] || qz_fail 'The configured Docker endpoint is not a Unix socket.'
    if qz_has "$file" docker_socket; then [[ $(qz_get "$file" docker_socket) == "$socket" ]] || qz_fail 'Use this installation’s original Docker socket.'; fi
    candidate=$(mktemp "$QZ_WORK/candidate.XXXXXX")
    qz_merge "$file" "$release" "$candidate"
    printf '{"codex_runtime_image":%s,"docker_socket":%s,"docker_socket_gid":%s}\n' "$(qz_quote "quazonai-codex:$project")" "$(qz_quote "$socket")" "$(stat -c %g -- "$socket")" > "$QZ_WORK/docker-config.json"
    qz_merge "$candidate" "$QZ_WORK/docker-config.json" "$candidate"
    qz_codex_prepare "$candidate" "$bundle"
    for field in image runtime_image; do
        image=$(qz_get "$release" "$field")
        if [[ "$image" != sha256:* ]]; then docker pull "$image"; fi
        revision=$(docker image inspect --format '{{index .Config.Labels "org.opencontainers.image.revision"}}' "$image")
        [[ "$revision" == "$(qz_get "$release" revision)" ]] || qz_fail "$field source revision does not match the deployment manifest."
    done
    destination="$root/releases/$(qz_get "$release" version)"; stored="$destination/deployment"
    if [[ -e "$stored/release.json" ]]; then
        qz_manifest "$stored/release.json"
        [[ $(qz_raw "$stored/release.json" '') == "$(qz_raw "$release" '')" ]] || qz_fail 'An installed version cannot be replaced with different content.'
    fi
    qz_directory "$destination"
    if [[ $(realpath -m -- "$stored") != "$(realpath -e -- "$bundle")" ]]; then
        qz_directory "$stored"
        for name in "${QZ_BUNDLE_FILES[@]}"; do [[ "$name" == release.json ]] || cp -- "$bundle/$name" "$stored/$name"; done
        cp -- "$release" "$stored/release.json"
    fi
    if [[ ! -e "$destination/bin" ]]; then
        rm -rf -- "$destination/bin.partial"; mkdir -- "$destination/bin.partial"
        container=$(docker create "$(qz_get "$release" image)")
        (
            trap 'docker rm "$container" >/dev/null' EXIT
            docker cp "$container:/opt/quazonai/bin/." "$destination/bin.partial"
            qz_native_binaries "$destination/bin.partial"
            mv -- "$destination/bin.partial" "$destination/bin"
        )
    else qz_native_binaries "$destination/bin"; fi
    printf '{"bundle":%s}\n' "$(qz_quote "$stored")" > "$QZ_WORK/bundle-config.json"
    qz_merge "$candidate" "$QZ_WORK/bundle-config.json" "$candidate"
    mkdir -p -- "$QZ_WORK/units"
    qz_worker_unit "$candidate" > "$QZ_WORK/units/$(qz_unit "$candidate")"
    systemd-analyze --user verify "$QZ_WORK/units/$(qz_unit "$candidate")"
    sync -f -- "$destination"
    QZ_PREPARED=$candidate
}
qz_install_stopped() {
    local file=$1 container observation load active enabled
    container=$(qz_compose "$file" ps --all --quiet app)
    if [[ -n "$container" ]]; then
        [[ "$container" != *$'\n'* ]] || qz_fail 'Ambiguous application containers prevent initial-install migration.'
        observation=$(docker inspect --format '{{.State.Status}} {{.State.Running}} {{.State.Restarting}} {{.HostConfig.RestartPolicy.Name}}' "$container")
        [[ "$observation" == 'created false false no' ]] || qz_fail 'Existing app has no completed-migration marker; preserve and reconcile before migration.'
    fi
    load=$(systemctl --user show "$(qz_unit "$file")" --property=LoadState --value)
    [[ "$load" != not-found ]] || return 0
    active=$(systemctl --user show "$(qz_unit "$file")" --property=ActiveState --value)
    enabled=$(systemctl --user show "$(qz_unit "$file")" --property=UnitFileState --value)
    [[ "$load" == loaded && ( "$active" == inactive || "$active" == failed ) && "$enabled" == disabled ]] || qz_fail 'An existing Worker is not stopped and disabled; initial migration was not repeated.'
}
qz_initialize_state() {
    local state="$(qz_get "$1" root)/data/state"
    if [[ ! -e "$state" ]]; then qz_compose "$1" run --rm --no-deps app init-state; fi
    [[ -f "$state/master.key" && -f "$state/session-key.ref" && -d "$state/secrets" && -d "$state/artifacts" ]] || qz_fail 'State initialization is incomplete. Preserve it and recover original keys; initialization was not repeated.'
    sync -f -- "$state"
}
qz_backup() {
    local file=$1 root destination suffix
    root=$(qz_get "$file" root); suffix=$(od -An -N3 -tx1 /dev/urandom | tr -d ' \n')
    destination="$root/backups/$(date +%Y%m%dT%H%M%S)-$suffix"
    [[ ! -e "$destination" ]] || qz_fail 'Recovery point already exists.'
    qz_directory "$destination"
    qz_compose "$file" exec -T database pg_dump -U quazonai -d quazonai -Fc > "$destination/database.dump"
    cp -- "$root/data/state/master.key" "$destination/master.key"
    tar --exclude=data/state/master.key -czf "$destination/data.tar.gz" -C "$root" data
    qz_save "$destination/installation.json" "$file"
    sync -f -- "$destination"
    printf '%s\n' "$destination" > "$QZ_WORK/recovery-path"
    printf 'Recovery point: %s; move master.key to separate protected storage.\n' "$destination" || :
}
qz_shutdown_and_backup() (
    local file=$1 phase=$2 root
    root=$(qz_get "$file" root)
    # A failed pre-DDL shutdown can restore only the unchanged old release.
    # A failed migration/configuration never enters this handler.
    trap 'status=$?; trap - EXIT; if [[ "$status" != 0 && "$phase" == preparing ]]; then
      if qz_resume "$file" && qz_restarts "$file" true; then rm -f -- "$root/pending.json"; sync -f -- "$root"; fi
    fi; exit "$status"' EXIT
    qz_restarts "$file" false
    qz_compose "$file" stop app
    qz_require_idle "$file"
    systemctl --user disable --now "$(qz_unit "$file")"
    qz_require_idle "$file"
    if qz_has "$file" codex_image; then qz_codex_require_stopped "$file" true; fi
    if [[ "$phase" == preparing ]]; then qz_backup "$file"; fi
    trap - EXIT
)
qz_deploy() {
    local root=$1 release="$QZ_BUNDLE/release.json" pending="$1/pending.json" file intent phase name password project codex_home
    qz_manifest "$release"; qz_preflight; qz_lock "$root"
    if [[ -e "$root/installation.json" ]]; then
        qz_configuration "$root"; file=$QZ_CONFIG
        for name in schema_version version revision image runtime_image codex_image codex_version database_image; do
            [[ $(qz_raw "$file" "$name") == "$(qz_raw "$release" "$name")" ]] || qz_fail 'Existing installation: use update.sh or the target release installer to change versions.'
        done
        if [[ ! -e "$pending" ]]; then
            if [[ -L "$root/current" ]]; then qz_verify_console "$file"; qz_verify_worker "$file"; printf 'Already installed: %s\n' "$(qz_get "$file" version)"; return; fi
            printf '{"operation":"install","phase":"migrating","version":%s}\n' "$(qz_raw "$release" version)" | qz_atomic_text "$pending"
        fi
        [[ $(qz_get "$pending" operation) == install ]] || qz_fail 'An update is pending; retry that original target.'
        phase=migrating; if qz_has "$pending" phase; then phase=$(qz_get "$pending" phase); fi
        [[ "$phase" == migrating || "$phase" == starting ]] || qz_fail 'Unknown initial-install phase; preserve its recovery record.'
        if [[ "$phase" == starting || -L "$root/current" ]]; then qz_start_prepared "$root" "$file"; return; fi
    else
        [[ ! -e "$pending" ]] || qz_fail 'Preserve and reconcile the interrupted installation before retrying.'
        for name in data current releases; do [[ ! -e "$root/$name" && ! -L "$root/$name" ]] || qz_fail 'Existing deployment files have no manifest; restore the original identity.'; done
        qz_ports "$QZ_PORT" "$QZ_DATABASE_PORT"
        qz_available_ports "$QZ_PORT" "$QZ_DATABASE_PORT"
        # Preserve the original path identity exactly; this is not an artifact checksum.
        project=$(printf '%s' "$root" | sha256sum); project="quazonai-${project:0:12}"
        for name in container volume network; do
            local command=(docker "$name" ls --quiet --filter "label=com.docker.compose.project=$project")
            if [[ "$name" == container ]]; then command+=(--all); fi
            [[ -z $("${command[@]}") ]] || qz_fail 'Existing Docker resources belong to this path. Restore the original manifest and keys; no new identity was created.'
        done
        password=$(od -An -N32 -tx1 /dev/urandom | tr -d ' \n')
        [[ "$password" =~ ^[0-9a-f]{64}$ ]] || qz_fail 'Could not generate an installation secret.'
        codex_home=$(realpath -m -- "$(qz_expand_path "${QZ_CODEX_HOME:-$root-codex}")")
        printf '{"root":%s,"uid":%s,"gid":%s,"home":%s,"codex_home":%s,"path":%s,"unit_directory":%s,"port":%s,"database_port":%s,"password":%s,"project":%s,"bundle":%s}\n' \
            "$(qz_quote "$root")" "$(id -u)" "$(id -g)" "$(qz_quote "$HOME")" "$(qz_quote "$codex_home")" "$(qz_quote "$PATH")" \
            "$(qz_quote "${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user")" "$((10#$QZ_PORT))" "$((10#$QZ_DATABASE_PORT))" "$(qz_quote "$password")" "$(qz_quote "$project")" "$(qz_quote "$QZ_BUNDLE")" > "$QZ_WORK/new-config.json"
        file="$QZ_WORK/install-config.json"; qz_merge "$release" "$QZ_WORK/new-config.json" "$file"
        qz_validate_paths "$file"; qz_directory "$codex_home"
        qz_save "$root/installation.json" "$file"
        printf '{"operation":"install","phase":"migrating","version":%s}\n' "$(qz_raw "$release" version)" | qz_atomic_text "$pending"
    fi
    qz_prepare "$QZ_BUNDLE" "$file"; file=$QZ_PREPARED
    qz_save "$root/installation.json" "$file"; qz_directory "$root/data"
    qz_compose "$file" up -d --wait --wait-timeout 120 database
    qz_install_stopped "$file"; qz_initialize_state "$file"
    qz_compose "$file" run --rm --no-deps app migrate
    qz_configure_worker "$file"
    qz_compose "$file" up --no-start --no-deps app
    printf '{"operation":"install","phase":"starting","version":%s}\n' "$(qz_raw "$file" version)" | qz_atomic_text "$pending"
    qz_start_prepared "$root" "$file"
}
qz_apply_update() {
    local root=$1 old candidate pending="$1/pending.json" phase=preparing
    qz_preflight; qz_lock "$root"; qz_configuration "$root"
    old="$QZ_WORK/old.json"; qz_save "$old" "$QZ_CONFIG"
    if [[ -e "$pending" ]]; then
        [[ $(qz_get "$pending" operation) == update && $(qz_get "$pending" target.version) == "$(qz_get "$QZ_BUNDLE/release.json" version)" ]] || qz_fail 'Retry the interrupted installation or original pending update target.'
        qz_raw "$pending" previous | qz_atomic_text "$old"
        phase=migrating; if qz_has "$pending" phase; then phase=$(qz_get "$pending" phase); fi
    fi
    qz_version "$(qz_get "$old" version)"; qz_manifest "$QZ_BUNDLE/release.json"
    [[ $(qz_precedence "$(qz_get "$QZ_BUNDLE/release.json" version)" "$(qz_get "$old" version)") != -1 ]] || qz_fail 'Application updates cannot downgrade; use an explicit matching cold restore.'
    qz_prepare "$QZ_BUNDLE" "$old"; candidate=$QZ_PREPARED
    [[ $(qz_get "$old" database_image) == "$(qz_get "$candidate" database_image)" ]] || qz_fail 'Application updates do not upgrade PostgreSQL.'
    if [[ ! -e "$pending" && $(qz_get "$old" version) == "$(qz_get "$candidate" version)" ]]; then
        qz_verify_console "$old"; qz_verify_worker "$old"; printf 'Already active: %s\n' "$(qz_get "$old" version)"; return
    fi
    if [[ -e "$pending" ]]; then [[ $(qz_raw "$candidate" '') == "$(qz_raw "$pending" target)" ]] || qz_fail 'Pending target changed; preserve its original bundle and configuration.'; fi
    [[ "$phase" == preparing || "$phase" == migrating || "$phase" == starting ]] || qz_fail 'Unknown pending phase; preserve its recovery record.'
    if [[ "$phase" == starting ]]; then qz_start_prepared "$root" "$candidate"; return; fi
    if [[ -e "$pending" && "$phase" == preparing ]]; then qz_resume "$old"; qz_restarts "$old" true; fi
    qz_require_idle "$old"
    if qz_has "$old" codex_image; then qz_codex_require_stopped "$old" true; fi
    if [[ ! -e "$pending" ]]; then
        printf '{"operation":"update","phase":"preparing","previous":%s,"target":%s,"backup":null}\n' "$(qz_raw "$old" '')" "$(qz_raw "$candidate" '')" | qz_atomic_text "$pending"
    fi
    qz_shutdown_and_backup "$old" "$phase"
    if [[ "$phase" == preparing ]]; then
        printf '{"phase":"migrating","backup":%s}\n' "$(qz_quote "$(cat "$QZ_WORK/recovery-path")")" > "$QZ_WORK/pending-patch.json"
        qz_merge "$pending" "$QZ_WORK/pending-patch.json" "$pending"
    fi
    qz_compose "$candidate" run --rm --no-deps app migrate
    qz_configure_worker "$candidate"
    qz_compose "$candidate" up --no-start --no-deps app
    printf '{"phase":"starting"}\n' > "$QZ_WORK/pending-patch.json"
    qz_merge "$pending" "$QZ_WORK/pending-patch.json" "$pending"
    qz_start_prepared "$root" "$candidate"
}
qz_unpack() {
    local archive=$1 destination=$2 names expected name type
    names=$(tar -tzf "$archive" | LC_ALL=C sort)
    expected=$(printf '%s\n' "${QZ_BUNDLE_FILES[@]}" | LC_ALL=C sort)
    [[ "$names" == "$expected" ]] || qz_fail 'Unexpected deployment bundle files.'
    # No archive path is extracted. Only exact named regular-file streams are copied.
    [[ -z $(tar -tvzf "$archive" | LC_ALL=C awk 'substr($0,1,1)!="-" {print "invalid"}') ]] || qz_fail 'Bundle members must be regular files.'
    mkdir -p -- "$destination"
    for name in "${QZ_BUNDLE_FILES[@]}"; do
        tar -xOzf "$archive" "$name" > "$destination/$name"
        [[ $(wc -c < "$destination/$name") -le 512000 ]] || qz_fail 'Unexpectedly large deployment bundle member.'
    done
    qz_manifest "$destination/release.json"
}
qz_download_update() {
    local root=$1 version=$2
    qz_version "$version"; qz_configuration "$root"
    [[ $(qz_precedence "$version" "$(qz_get "$QZ_CONFIG" version)") != -1 ]] || qz_fail 'Application updates cannot downgrade; use an explicit matching cold restore.'
    curl --fail --location --proto '=https' --proto-redir '=https' --tlsv1.2 --retry 3 --connect-timeout 15 --max-time 600 --max-filesize 3000000 \
        --output "$QZ_WORK/quazonai-deploy.tar.gz" "https://github.com/zhengui666/QuaZonai/releases/download/$version/quazonai-deploy.tar.gz"
    qz_unpack "$QZ_WORK/quazonai-deploy.tar.gz" "$QZ_WORK/deployment"
    [[ $(qz_get "$QZ_WORK/deployment/release.json" version) == "$version" ]] || qz_fail 'Downloaded bundle does not match the requested version.'
    bash "$QZ_WORK/deployment/manage.sh" apply-update --directory "$root"
}
qz_runtime() {
    local root=$1 operation=$2 config_path=$3 file release n i
    [[ "$operation" == doctor || "$operation" == serve ]] || qz_fail 'runtime requires doctor|serve and --config /absolute/runtime.json'
    qz_configuration "$root"; file=$QZ_CONFIG
    [[ ! -e "$root/pending.json" ]] || qz_fail 'Complete the recorded application update before selecting its Runtime.'
    release="$(qz_get "$file" bundle)/release.json"; qz_manifest "$release"
    [[ $(qz_type "$config_path" images) == array ]] || qz_fail 'Runtime images must be an array.'
    n=$(qz_json length images "$config_path"); ((n>0)) || qz_fail 'Runtime images must not be empty.'
    for ((i=0;i<n;i++)); do
        [[ $(qz_get "$config_path" "images.$i.image_ref") == "$(qz_get "$release" runtime_image)" ]] || qz_fail 'Runtime images must match this release’s runtime_image digest.'
    done
    # Do not require Docker/network health: existing gateway journals stay readable offline.
    local binary="$root/releases/$(qz_get "$file" version)/bin/runtime"
    rm -rf -- "$QZ_WORK"; trap - EXIT
    exec "$binary" "$operation" --config "$config_path"
}
qz_apply_config() {
    local root=$1 runtime_targets=${2:-} downstream_targets=${3:-} resume=${4:-false} file candidate checkpoint field source
    qz_preflight; qz_lock "$root"; qz_configuration "$root"; file=$QZ_CONFIG
    [[ ! -e "$root/pending.json" ]] || qz_fail 'Complete the recorded installation/update first.'
    if [[ "$resume" == true ]]; then
        qz_resume "$file" false; qz_verify_worker "$file"; qz_verify_console "$file"
        systemctl --user enable "$(qz_unit "$file")"; qz_restarts "$file" true; return
    fi
    qz_compose "$file" config --quiet
    candidate="$QZ_WORK/config-candidate.json"; qz_save "$candidate" "$file"
    for field in runtime_targets downstream_targets; do
        source=$runtime_targets; if [[ "$field" == downstream_targets ]]; then source=$downstream_targets; fi
        if [[ -n "$source" ]]; then
            [[ $(qz_type "$source" '') == array ]] || qz_fail "$field must contain a JSON array."
            printf '{"%s":%s}\n' "$field" "$(qz_raw "$source" '')" > "$QZ_WORK/config-patch.json"
            qz_merge "$candidate" "$QZ_WORK/config-patch.json" "$candidate"
        fi
    done
    qz_require_idle "$file"; qz_codex_require_stopped "$file" true
    qz_directory "$root/backups"; checkpoint=$(mktemp -d "$root/backups/runtime-config-XXXXXX")
    qz_save "$checkpoint/installation.json" "$file"; sync -f -- "$root/backups"
    printf 'Configuration checkpoint: %s\n' "$checkpoint"
    (
        trap 'status=$?; trap - EXIT; if [[ "$status" != 0 ]]; then qz_resume "$file" && qz_restarts "$file" true; fi; exit "$status"' EXIT
        qz_restarts "$file" false; qz_compose "$file" stop app
        qz_require_idle "$file"; qz_codex_require_stopped "$file" true
        systemctl --user disable --now "$(qz_unit "$file")"; qz_require_idle "$file"
        trap - EXIT
    )
    qz_save "$root/installation.json" "$candidate"; qz_configure_worker "$candidate"
    qz_compose "$candidate" up --no-start --no-deps app; qz_restarts "$candidate" false
    printf 'Configuration prepared; starting the original installation\n'
    qz_resume "$candidate" false; qz_verify_worker "$candidate"; qz_verify_console "$candidate"
    systemctl --user enable "$(qz_unit "$candidate")"; qz_restarts "$candidate" true
}
qz_source_path() {
    local path=$1 file=$2 output=$3 component reserved root codex units socket uid
    [[ "$path" == /* && "$path" != *[$'\001'-$'\037'$'\177':,\"\\]* && "/$path/" != */../* ]] || qz_fail 'Source mounts require absolute paths without traversal, control characters, colons, commas, quotes or backslashes.'
    # Lexical normalization strips trailing/repeated slash and dot components
    # without dereferencing a link before the component-by-component check.
    path=$(realpath -ms -- "$path")
    component=$path
    while [[ "$component" != / ]]; do [[ ! -L "$component" ]] || qz_fail 'Source mount paths cannot contain symlinks.'; component=$(dirname -- "$component"); done
    path=$(realpath -e -- "$path")
    [[ "$path" != / && "$path" != /tmp ]] || qz_fail 'Source mounts cannot expose filesystem roots.'
    local forbidden=(/opt/quazonai /usr /proc /sys /dev /etc /bin /sbin /lib /lib64 /root /run)
    for component in root codex_home unit_directory docker_socket; do if qz_has "$file" "$component"; then forbidden+=("$(realpath -m -- "$(qz_get "$file" "$component")")"); fi; done
    for reserved in "${forbidden[@]}"; do
        [[ "$path" != "$reserved" && "$path" != "$reserved/"* && "$reserved" != "$path/"* ]] || qz_fail 'Source mounts cannot overlap container tools, system paths or installation state.'
    done
    [[ $(stat -c %u -- "$path") == "$(qz_get "$file" uid)" ]] || qz_fail 'Source mounts must belong to the installation owner.'
    if [[ "$output" == true ]]; then [[ -d "$path" && -w "$path" && -x "$path" ]] || qz_fail 'Output parent must be an existing writable owner directory.'
    else [[ -d "$path" || -f "$path" ]] || qz_fail 'Source input must be an owner-managed directory or regular file.'; fi
    printf '%s' "$path"
}
qz_source_container() {
    local file=$1 release=$2 network=$3 invocation=$4 inventory=${5:-false} suffix= uid gid
    if [[ "$inventory" == true ]]; then suffix=-inventory; fi
    uid=$(qz_get "$file" uid); gid=$(qz_get "$file" gid)
    QZ_SOURCE_COMMAND=(docker run --rm --init --no-healthcheck --read-only --name "quazonai-source-$invocation$suffix"
        --label "io.quazonai.source.invocation=$invocation" --label "io.quazonai.source.installation=$(qz_get "$file" project)" --label "io.quazonai.source.owner=$uid"
        --network "$network" --cap-drop ALL --security-opt no-new-privileges:true --user "$uid:$gid" --workdir /tmp
        --tmpfs /tmp:rw,nosuid,nodev,noexec,size=268435456,mode=1777 --cpus 2 --memory 4g --memory-swap 4g --pids-limit 256
        --env QZ_OPERATOR_INSTALLED=1 --env "QZ_OPERATOR_VERSION=$(qz_get "$release" version)" --env "QZ_OPERATOR_REVISION=$(qz_get "$release" revision)"
        --env "QZ_OPERATOR_IMAGE=$(qz_get "$release" image)" --entrypoint /usr/bin/python3)
}
qz_source() {
    local root="$HOME/.local/share/quazonai" output= invocation= file release path other destination= help_only=false n i j selected=-1 operation plugin cap found network=none endpoint options
    local inputs=() mounts=() modes=() arguments=() destinations=()
    while (($#)); do
        case "$1" in
          --help|-h) printf 'Usage: bash manage.sh source [--directory PATH] [--read-only PATH] [--output-parent PATH] [--invocation-id HEX] -- OPERATION [ARGUMENTS]\nRuns the installed image’s source tools; inputs and outputs need explicit owner-managed mounts.\n'; return;;
          --directory|--read-only|--output-parent|--invocation-id) (($#>=2)) || qz_fail "Missing value for $1"; case "$1" in --directory) root=$2;; --read-only) inputs+=("$2");; --output-parent) output=$2;; --invocation-id) invocation=$2;; esac; shift 2;;
          --) shift; arguments=("$@"); break;;
          *) arguments=("$@"); break;;
        esac
    done
    root=$(realpath -m -- "$(qz_expand_path "$root")"); qz_configuration "$root"; file=$QZ_CONFIG
    [[ ! -e "$root/pending.json" ]] || qz_fail 'Complete the recorded application update before selecting its source tools.'
    release="$(qz_get "$file" bundle)/release.json"; qz_manifest "$release"
    ((${#arguments[@]})) || qz_fail 'Provide a source operation.'
    for path in "${arguments[@]}"; do [[ "$path" != --native-bin && "$path" != --native-bin=* ]] || qz_fail 'Installed tools always use the matching packaged native binary.'; done
    if [[ -z "$invocation" ]]; then invocation=$(od -An -N16 -tx1 /dev/urandom | tr -d ' \n'); fi
    [[ "$invocation" =~ ^[0-9a-f]{32}$ ]] || qz_fail 'Invocation identity must be 32 lowercase hexadecimal characters.'
    printf 'Source invocation: %s\n' "$invocation" >&2
    for path in "${inputs[@]}"; do mounts+=("$(qz_source_path "$path" "$file" false)"); modes+=(ro); done
    if [[ -n "$output" ]]; then output=$(qz_source_path "$output" "$file" true); mounts+=("$output"); modes+=(rw); fi
    for ((i=0;i<${#mounts[@]};i++)); do for ((j=0;j<i;j++)); do
        path=${mounts[i]}; other=${mounts[j]}
        [[ "$path" != "$other" && "$path" != "$other/"* && "$other" != "$path/"* ]] || qz_fail 'Source input and output mounts must not overlap.'
    done; done
    operation=${arguments[0]}
    if [[ ${arguments[${#arguments[@]}-1]} == --help || ${arguments[${#arguments[@]}-1]} == -h ]]; then help_only=true; fi
    if [[ "$help_only" == false && ( "$operation" == download || "$operation" == freeze || "$operation" == convert || "$operation" == prepare ) ]]; then
        for ((i=0;i<${#arguments[@]};i++)); do
            if [[ ${arguments[i]} == --output=* ]]; then destinations+=("${arguments[i]#--output=}");
            elif [[ ${arguments[i]} == --output ]] && ((i+1<${#arguments[@]})); then destinations+=("${arguments[i+1]}"); fi
        done
        [[ -n "$output" && ${#destinations[@]} == 1 ]] || qz_fail 'Writing needs exactly one --output inside an explicit --output-parent mount.'
        destination=${destinations[0]}
        [[ "$destination" == "$output/"* && "$destination" == "$(realpath -ms -- "$destination")" && ! -e "$destination" && ! -L "$destination" ]] || qz_fail 'Source output must be a new absolute child of the mounted output parent.'
        path=$(dirname -- "$destination")
        while [[ "$path" != "$output" ]]; do [[ ! -L "$path" ]] || qz_fail 'Source output paths cannot contain symlinks.'; path=$(dirname -- "$path"); done
    fi
    endpoint=${DOCKER_HOST:-$(docker context inspect --format '{{.Endpoints.docker.Host}}')}
    [[ "$endpoint" == unix://* && $(realpath -m -- "${endpoint#unix://}") == "$(realpath -m -- "$(qz_get "$file" docker_socket)")" ]] || qz_fail 'Source tools require the original local Docker socket.'
    qz_docker_mapping
    [[ $(docker image inspect --format '{{index .Config.Labels "org.opencontainers.image.revision"}}' "$(qz_get "$release" image)") == "$(qz_get "$release" revision)" ]] || qz_fail 'Source tools image revision does not match this installed release.'
    local prefix=(-E -s -B /opt/quazonai/operator/source_plugins.py)
    if [[ "$operation" != plugins && "$help_only" == false ]]; then
        ((${#arguments[@]}>=2)) || qz_fail 'Provide an installed source plugin.'; plugin=${arguments[1]}
        qz_source_container "$file" "$release" none "$invocation" true
        "${QZ_SOURCE_COMMAND[@]}" "$(qz_get "$release" image)" "${prefix[@]}" plugins > "$QZ_WORK/inventory.json"
        [[ $(qz_type "$QZ_WORK/inventory.json" '') == array ]] || qz_fail 'Installed inventory is not an array.'
        n=$(qz_json length '' "$QZ_WORK/inventory.json"); ((n>=1 && n<=256)) || qz_fail 'Installed inventory is invalid.'
        for ((i=0;i<n;i++)); do
            if [[ $(qz_get "$QZ_WORK/inventory.json" "$i.id") == "$plugin" ]]; then [[ "$selected" == -1 ]] || qz_fail 'Ambiguous plugin identity.'; selected=$i; fi
        done
        [[ "$selected" != -1 ]] || qz_fail 'Installed plugin is unavailable.'
        n=$(qz_json length "$selected.capabilities" "$QZ_WORK/inventory.json"); found=false
        local capabilities=()
        for ((i=0;i<n;i++)); do
            [[ $(qz_type "$QZ_WORK/inventory.json" "$selected.capabilities.$i") == string ]] || qz_fail 'Invalid plugin capability.'
            cap=$(qz_get "$QZ_WORK/inventory.json" "$selected.capabilities.$i"); capabilities+=("$cap"); if [[ "$cap" == "$operation" ]]; then found=true; fi
        done
        [[ "$found" == true ]] || qz_fail 'This installed plugin does not support the operation.'
        [[ $(qz_type "$QZ_WORK/inventory.json" "$selected.public_network_operations") == array ]] || qz_fail 'Invalid source network declaration.'
        n=$(qz_json length "$selected.public_network_operations" "$QZ_WORK/inventory.json")
        for ((i=0;i<n;i++)); do
            [[ $(qz_type "$QZ_WORK/inventory.json" "$selected.public_network_operations.$i") == string ]] || qz_fail 'Invalid source network declaration.'
            cap=$(qz_get "$QZ_WORK/inventory.json" "$selected.public_network_operations.$i"); found=false
            for other in "${capabilities[@]}"; do if [[ "$cap" == "$other" ]]; then found=true; fi; done
            [[ "$found" == true ]] || qz_fail 'Network declaration is not a capability.'
            if [[ "$cap" == "$operation" ]]; then network=bridge; fi
        done
        if [[ "$network" == bridge ]]; then case "$operation" in inspect|freeze|verify|convert|prepare) qz_fail 'Installed inspection/conversion/preparation must remain offline.';; esac; fi
    fi
    qz_source_container "$file" "$release" "$network" "$invocation"
    for ((i=0;i<${#mounts[@]};i++)); do path="type=bind,source=${mounts[i]},target=${mounts[i]}"; if [[ ${modes[i]} == ro ]]; then path+=,readonly; fi; QZ_SOURCE_COMMAND+=(--mount "$path"); done
    # Python belongs to the prebuilt source-tools image, never the host bootstrap.
    local image; image=$(qz_get "$release" image)
    rm -rf -- "$QZ_WORK"; trap - EXIT
    exec "${QZ_SOURCE_COMMAND[@]}" "$image" "${prefix[@]}" "${arguments[@]}"
}
qz_recover_access() {
    local root=$1 record=$2 file binary recovery_id database_url receipt key parent record_name record_stem
    qz_lock "$root"; qz_configuration "$root"; file=$QZ_CONFIG
    binary="$root/releases/$(qz_get "$file" version)/bin/server"
    [[ -f "$binary" ]] || qz_fail 'Restore the matching release binary first.'
    [[ $(qz_get "$file" bundle) == "$root/releases/$(qz_get "$file" version)/deployment" ]] || qz_fail 'Restored manifest must name its matching preserved release bundle.'
    record=$(realpath -m -- "$(qz_expand_path "$record")"); parent=$(dirname -- "$record")
    record_name=$(basename -- "$record"); record_stem=$record_name
    if [[ "$record_name" == *.* && "$record_name" != *. && "${record_name#.}" == *.* ]]; then record_stem=${record_name%.*};
    elif [[ "$record_name" != .* && "$record_name" == *.* && "$record_name" != *. ]]; then record_stem=${record_name%.*}; fi
    [[ -d "$parent" && $(stat -c %u -- "$parent") == "$(id -u)" && $(stat -c %a -- "$parent") == 700 && ! -L "$record" ]] || qz_fail 'Choose a recovery record in an existing mode-0700 owner directory.'
    if [[ -e "$record" ]]; then
        for key in root version revision; do [[ $(qz_raw "$record" "$key") == "$(qz_raw "$file" "$key")" ]] || qz_fail 'Recovery record belongs to a different installation or release.'; done
        recovery_id=$(qz_get "$record" recovery_id)
    else
        recovery_id=$(uuidgen --time-v7)
        [[ "$recovery_id" =~ ^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$ ]] || qz_fail 'uuidgen must produce a UUIDv7.'
        printf '{"root":%s,"version":%s,"revision":%s,"recovery_id":%s}\n' "$(qz_raw "$file" root)" "$(qz_raw "$file" version)" "$(qz_raw "$file" revision)" "$(qz_quote "$recovery_id")" | qz_atomic_text "$record"
    fi
    [[ "$recovery_id" =~ ^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$ ]] || qz_fail 'Recovery record requires its original UUIDv7.'
    printf 'Restored migration-owner DATABASE_URL (hidden): ' > /dev/tty
    IFS= read -r -s database_url < /dev/tty
    printf '\n' > /dev/tty
    [[ -n "$database_url" ]] || qz_fail 'A migration-owner database connection is required.'
    if ! DATABASE_URL="$database_url" "$binary" recover-access --recovery-id "$recovery_id" > "$QZ_WORK/receipt.json" 2> "$QZ_WORK/recover-error.log"; then
        unset database_url
        qz_save "$parent/$record_stem.error.log" "$QZ_WORK/recover-error.log"
        qz_fail 'Cutover not confirmed; inspect the private error file and retry this record.'
    fi
    unset database_url
    [[ $(qz_get "$QZ_WORK/receipt.json" schema_version) == 1 && $(qz_get "$QZ_WORK/receipt.json" recovery_id) == "$recovery_id" ]] || qz_fail 'Unexpected receipt; retain the original record for reconciliation.'
    local previous next
    previous=$(qz_get "$QZ_WORK/receipt.json" previous_epoch); next=$(qz_get "$QZ_WORK/receipt.json" new_epoch)
    [[ "$previous" =~ ^(0|[1-9][0-9]*)$ && "$next" =~ ^(0|[1-9][0-9]*)$ ]] || qz_fail 'Receipt epochs must be decimal integers.'
    LC_ALL=C awk -v a="$next" -v b="$previous" 'BEGIN { exit !(length(a)>length(b) || (length(a)==length(b) && ("x" a)>("x" b))) }' || qz_fail 'Receipt access epoch did not increase.'
    receipt="$parent/$record_stem.receipt.json"
    if [[ -e "$receipt" ]]; then [[ $(qz_raw "$receipt" '') == "$(qz_raw "$QZ_WORK/receipt.json" '')" ]] || qz_fail 'Receipt differs from saved outcome; do not create a replacement identity.'; fi
    qz_save "$receipt" "$QZ_WORK/receipt.json"
    printf 'Access cutover receipt: %s\n' "$receipt"
}
qz_help() {
    cat <<'HELP'
Usage: bash manage.sh COMMAND [VERSION] [--directory PATH] [OPTIONS]
Commands: deploy, update VERSION, apply-update, status, runtime doctor|serve,
          runtime-guides, source, apply-config, resume-config, recover-access RECORD
Options: --port PORT --database-port PORT --codex-home PATH --config FILE
         --runtime-targets FILE --downstream-targets FILE
Requires Bash 4.4+, curl, Docker Compose 2.20+, standard Linux utilities,
Git and a persistent systemd user manager; install/update never build images.
Use the original non-root owner and path. Preserve pending.json and backups.
HELP
}
qz_main() {
    local command=${1:-} version= root="$HOME/.local/share/quazonai" config= runtime_targets= downstream_targets= revision
    if [[ -z "$command" || "$command" == --help || "$command" == -h ]]; then qz_help; return; fi
    shift
    # Help does not create directories, configuration or credentials.
    if [[ "$command" != source ]]; then for argument in "$@"; do if [[ "$argument" == --help || "$argument" == -h ]]; then qz_help; return; fi; done; fi
    QZ_PORT=8081; QZ_DATABASE_PORT=55432; QZ_CODEX_HOME=
    if [[ "$command" != source ]]; then
        while (($#)); do
            case "$1" in
              --directory|--port|--database-port|--codex-home|--config|--runtime-targets|--downstream-targets)
                (($#>=2)) && [[ -n "$2" ]] || qz_fail "Missing value for $1"
                case "$1" in --directory) root=$2;; --port) QZ_PORT=$2;; --database-port) QZ_DATABASE_PORT=$2;; --codex-home) QZ_CODEX_HOME=$2;; --config) config=$2;; --runtime-targets) runtime_targets=$2;; --downstream-targets) downstream_targets=$2;; esac; shift 2;;
              -*) qz_fail "Unknown option: $1";;
              *) [[ -z "$version" ]] || qz_fail 'Unexpected positional argument.'; version=$1; shift;;
            esac
        done
    elif [[ ${1:-} == --help || ${1:-} == -h ]]; then qz_source "$@"; return; fi
    qz_require_bash
    [[ "$root" != *[$'\001'-$'\037'$'\177']* ]] || qz_fail 'Installation paths cannot contain control characters.'
    root=$(realpath -m -- "$(qz_expand_path "$root")")
    [[ "$root" != / && "$root" != "$(realpath -m -- "$HOME")" ]] || qz_fail 'Select a dedicated installation directory.'
    qz_ports "$QZ_PORT" "$QZ_DATABASE_PORT"
    QZ_WORK=$(mktemp -d "${TMPDIR:-/tmp}/quazonai-manage.XXXXXX")
    trap 'rm -rf -- "$QZ_WORK"' EXIT
    source "$QZ_BUNDLE/codex.sh"
    case "$command" in
      deploy) qz_deploy "$root";; apply-update) qz_apply_update "$root";;
      update) [[ -n "$version" ]] || qz_fail 'update requires an explicit release tag.'; qz_download_update "$root" "$version";;
      runtime) [[ -n "$config" ]] || qz_fail 'runtime requires --config FILE.'; qz_runtime "$root" "$version" "$(realpath -e -- "$(qz_expand_path "$config")")";;
      source) qz_source "$@";;
      apply-config) qz_apply_config "$root" "$runtime_targets" "$downstream_targets";; resume-config) qz_apply_config "$root" '' '' true;;
      runtime-guides) revision=$(qz_get "$QZ_BUNDLE/release.json" revision); [[ "$revision" =~ ^[0-9a-f]{40}$ ]] || qz_fail 'Invalid release revision.'
        for argument in scientific-runtime runtime-targets runtime-recovery access-cutover; do printf 'https://github.com/zhengui666/QuaZonai/blob/%s/.opensdlc/operations.md#%s\n' "$revision" "$argument"; done;;
      recover-access) [[ -n "$version" ]] || qz_fail 'recover-access requires a private record filename.'; qz_recover_access "$root" "$version";;
      status) qz_configuration "$root"; printf 'Recorded release: %s; http://localhost:%s\n' "$(qz_get "$QZ_CONFIG" version)" "$(qz_get "$QZ_CONFIG" port)"
        if [[ -e "$root/pending.json" ]]; then printf 'An interrupted operation exists: inspect pending.json privately; it contains deployment credentials.\n'; fi
        qz_compose "$QZ_CONFIG" ps; systemctl --user status --no-pager "$(qz_unit "$QZ_CONFIG")";;
      *) qz_fail "Unknown command: $command";;
    esac
}
if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then qz_main "$@"; fi
