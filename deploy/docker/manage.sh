#!/usr/bin/env bash
# Prebuilt Node application + official Durable writer, all inside Docker.
set -euo pipefail
umask 077
QZ_BUNDLE=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
QZ_BUNDLE_FILES=(manage.sh json.awk deploy.sh update.sh compose.yaml release.json README.md .env.example)
qz_fail() { printf 'Deployment failed: %s\n' "$*" >&2; exit 1; }
qz_get() { LC_ALL=C awk -v mode=get -v path="$2" -f "$QZ_BUNDLE/json.awk" "$1"; }
qz_raw() { LC_ALL=C awk -v mode=raw -v path="${2:-}" -f "$QZ_BUNDLE/json.awk" "$1"; }
qz_quote() { printf '%s\001' "$1" | LC_ALL=C awk -v mode=quote -v path='' -f "$QZ_BUNDLE/json.awk"; }
qz_save() {
    local temporary
    temporary=$(mktemp "$(dirname -- "$1")/.quazonai-write.XXXXXX")
    cat > "$temporary"
    chmod 600 "$temporary"; sync -f "$temporary"
    mv -fT -- "$temporary" "$1"; sync -f "$(dirname -- "$1")"
}
qz_version() { [[ "$1" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]] || qz_fail 'Use an explicit vMAJOR.MINOR.PATCH[-prerelease] release tag.'; }
qz_manifest() {
    [[ $(qz_get "$1" schema_version) == 5 ]] || qz_fail 'Use a schema-5 Node deployment bundle.'
    qz_version "$(qz_get "$1" version)"
    [[ $(qz_get "$1" revision) =~ ^[0-9a-f]{40}$ ]] || qz_fail 'Invalid release revision.'
    local field reference
    for field in image runtime_image; do
        reference=$(qz_get "$1" "$field")
        [[ "$reference" =~ ^(ghcr\.io/[a-zA-Z0-9/._-]+@)?sha256:[0-9a-f]{64}$ ]] || qz_fail "$field must identify a prebuilt image."
    done
}
qz_configuration() {
    QZ_CONFIG="$1/installation.json"
    [[ -f "$QZ_CONFIG" ]] || qz_fail "Original installation manifest missing: $QZ_CONFIG"
    [[ $(qz_get "$QZ_CONFIG" schema_version) == 5 ]] || qz_fail 'Unsupported installation format; create a fresh schema-5 installation.'
    [[ $(qz_get "$QZ_CONFIG" root) == "$1" && $(qz_get "$QZ_CONFIG" uid) == "$(id -u)" ]] || qz_fail 'Use the original installation owner and path.'
    qz_manifest "$QZ_CONFIG"
}
qz_preflight() {
    [[ $(uname -s)/$(uname -m) == Linux/x86_64 && $(id -u) != 0 ]] || qz_fail 'Run as the non-root Linux x86_64 installation owner.'
    local tool
    for tool in docker curl tar awk flock realpath sync od; do command -v "$tool" >/dev/null || qz_fail "Missing $tool"; done
    docker compose version >/dev/null
}
qz_lock() { mkdir -p "$1"; exec 9>>"$1/.deployment.lock"; flock -n 9 || qz_fail 'Another deployment operation is running.'; }
qz_compose() (
    local file=$1; shift
    export APP_IMAGE=$(qz_get "$file" image)
    export INSTALL_DIR=$(qz_get "$file" root)
    export WEB_PORT=$(qz_get "$file" port)
    export HOST_UID=$(qz_get "$file" uid) HOST_GID=$(qz_get "$file" gid)
    local args=(docker compose --project-name "$(qz_get "$file" project)" --env-file /dev/null -f "$(qz_get "$file" bundle)/compose.yaml")
    if [[ -f "$INSTALL_DIR/compose.override.yaml" ]]; then args+=(-f "$INSTALL_DIR/compose.override.yaml"); fi
    if [[ $(qz_get "$file" local_runtime_service 2>/dev/null || true) == nautilus ]]; then
        "${args[@]}" config --services | grep -qx nautilus || qz_fail 'The managed local nautilus service is missing from the owner Compose configuration.'
        local overlay="$QZ_WORK/runtime-image.yaml"
        printf 'services:\n  nautilus:\n    image: "%s"\n' "$(qz_get "$file" runtime_image)" > "$overlay"
        args+=(-f "$overlay")
    fi
    "${args[@]}" "$@"
)
# Opt-in only: existing local service settings remain in the owner's override.
# Old workers have no atomic drain API; never turn Docker stop into cancellation.
qz_runtime_stopped() {
    local file=$1 id
    qz_compose "$file" config --services | grep -qx nautilus || qz_fail 'Local runtime requires the existing nautilus Compose service.'
    id=$(qz_compose "$file" ps --all --quiet nautilus)
    [[ -n "$id" && "$id" != *$'\n'* ]] || qz_fail 'Inspect the original single nautilus container before enrolling/updating its runtime.'
    [[ $(docker inspect --format '{{.State.Running}}' "$id") == false ]] || qz_fail 'Local nautilus is running. Finish and reconcile its work, then safely stop it before updating; this command never cancels remote work.'
}
qz_runtime_verify() {
    local file=$1 id expected
    [[ $(qz_get "$file" local_runtime_service 2>/dev/null || true) == nautilus ]] || return 0
    id=$(qz_compose "$file" ps --quiet nautilus)
    [[ -n "$id" && "$id" != *$'\n'* ]] || qz_fail 'Expected one running local nautilus container.'
    expected=$(docker image inspect --format '{{.Id}}' "$(qz_get "$file" runtime_image)")
    [[ $(docker inspect --format '{{.Image}}' "$id") == "$expected" ]] || qz_fail 'Running local runtime differs from the release image.'
    [[ $(docker inspect --format '{{.State.Running}}' "$id") == true ]] || qz_fail 'Local runtime did not remain running.'
}
qz_prepare() {
    local root=$1 release="$QZ_BUNDLE/release.json" file=$2 directory field image name
    qz_manifest "$release"
    # A remote Nautilus instance is configured separately. Updating the local
    # application only pulls its optional runtime image after explicit local enrollment.
    for field in image; do
        image=$(qz_get "$release" "$field")
        if [[ "$image" != sha256:* ]]; then docker pull "$image"; fi
        [[ $(docker image inspect --format '{{index .Config.Labels "org.opencontainers.image.revision"}}' "$image") == "$(qz_get "$release" revision)" ]] || qz_fail 'Image source differs from the release manifest.'
    done
    directory="$root/releases/$(qz_get "$release" version)/deployment"
    if [[ -e "$directory/release.json" ]]; then
        [[ $(qz_raw "$directory/release.json") == "$(qz_raw "$release")" ]] || qz_fail 'An installed version cannot be overwritten.'
    else
        mkdir -p "$directory"
        for name in "${QZ_BUNDLE_FILES[@]}"; do cp -- "$QZ_BUNDLE/$name" "$directory/$name"; done
    fi
    printf '{"bundle":%s}\n' "$(qz_quote "$directory")" > "$QZ_WORK/bundle.json"
    LC_ALL=C awk -v mode=merge -v path='' -f "$QZ_BUNDLE/json.awk" "$file" "$release" > "$QZ_WORK/release-config.json"
    LC_ALL=C awk -v mode=merge -v path='' -f "$QZ_BUNDLE/json.awk" "$QZ_WORK/release-config.json" "$QZ_WORK/bundle.json" > "$QZ_WORK/candidate.json"
    QZ_PREPARED="$QZ_WORK/candidate.json"
    if [[ ${QZ_LOCAL_RUNTIME:-false} == true ]]; then
        printf '{"local_runtime_service":"nautilus"}\n' > "$QZ_WORK/runtime-config.json"
        LC_ALL=C awk -v mode=merge -v path='' -f "$QZ_BUNDLE/json.awk" "$QZ_PREPARED" "$QZ_WORK/runtime-config.json" > "$QZ_WORK/runtime-candidate.json"
        QZ_PREPARED="$QZ_WORK/runtime-candidate.json"
    fi
    if [[ $(qz_get "$QZ_PREPARED" local_runtime_service 2>/dev/null || true) == nautilus ]]; then
        image=$(qz_get "$release" runtime_image)
        if [[ "$image" != sha256:* ]]; then docker pull "$image"; fi
        [[ $(docker image inspect --format '{{index .Config.Labels "org.opencontainers.image.revision"}}' "$image") == "$(qz_get "$release" revision)" ]] || qz_fail 'Runtime revision differs from the release.'
        [[ $(docker image inspect --format '{{index .Config.Labels "org.opencontainers.image.version"}}' "$image") == "$(qz_get "$release" version)" ]] || qz_fail 'Runtime version differs from the release.'
    fi
}
qz_verify() {
    local origin="http://localhost:$(qz_get "$1" port)" code
    code=$(curl --silent --show-error --noproxy '*' --max-time 10 --output /dev/null --write-out '%{http_code}' "$origin/health/live")
    [[ "$code" == 204 ]] || qz_fail 'Node API liveness failed.'
    curl --fail --silent --show-error --noproxy '*' --max-time 10 "$origin/" > "$QZ_WORK/frontend.html"
    grep -qi '<!doctype html>' "$QZ_WORK/frontend.html" || qz_fail 'Production Web assets were not served.'
    qz_runtime_verify "$1"
}
qz_start() {
    local file=$1 root pending; shift
    root=$(qz_get "$file" root); pending="$root/pending.json"
    if [[ ! -f "$pending" || $(qz_get "$pending" operation) != install ]]; then
        qz_compose "$file" run --rm --no-deps app maintenance exists
    fi
    qz_compose "$file" up "$@" -d --wait --wait-timeout 120; qz_verify "$file"
}
qz_stop() {
    local file=$1 id observation
    # SIGTERM closes HTTP admission and drains/suspends the programmatic host.
    qz_compose "$file" stop --timeout 90 app
    id=$(qz_compose "$file" ps --all --quiet app)
    [[ -n "$id" ]] || qz_fail 'Original app is absent; inspect original persisted tasks before maintenance.'
    observation=$(docker inspect --format '{{.State.Running}} {{.State.ExitCode}}' "$id")
    [[ "$observation" == 'false 0' || "$observation" == 'false 143' ]] || qz_fail 'App did not stop gracefully; reconcile owned native/remote operations. Local exit is not cancellation.'
}
qz_idle() { qz_compose "$1" run --rm --no-deps app maintenance; }
qz_backup() {
    local file=$1 root destination
    root=$(qz_get "$file" root)
    mkdir -p "$root/backups"
    destination=$(mktemp -d "$root/backups/$(date +%Y%m%dT%H%M%S)-XXXXXX")
    printf '%s\n' sqlite > "$destination/storage-format"
    # Includes original config, modules, env, keys, artifacts, SQLite/WAL and journals.
    if cp --reflink=always "$destination/storage-format" "$destination/.reflink-probe" 2>/dev/null; then
        rm -f "$destination/.reflink-probe"
        cp -a --reflink=auto "$root/data" "$destination/data"
    else
        rm -f "$destination/.reflink-probe"
        tar -czf "$destination/data.tar.gz" -C "$root" data
    fi
    cp "$file" "$destination/installation.json"
    if [[ -e "$root/compose.override.yaml" ]]; then cp "$root/compose.override.yaml" "$destination/compose.override.yaml"; fi
    sync -f "$destination"
    QZ_BACKUP="$destination"
    printf 'Stopped-writer recovery point: %s\n' "$destination"
}
qz_activate() {
    local root=$1 file=$2
    qz_start "$file" "${@:3}"
    qz_save "$root/installation.json" < "$file"
    ln -sfn "releases/$(qz_get "$file" version)" "$root/current.next"
    mv -fT "$root/current.next" "$root/current"
    rm -f "$root/pending.json"; sync -f "$root"
    printf 'Active %s: http://localhost:%s\n' "$(qz_get "$file" version)" "$(qz_get "$file" port)"
}
qz_deploy() {
    local root=$1 file project name
    qz_manifest "$QZ_BUNDLE/release.json"
    if [[ -e "$root/installation.json" ]]; then
        qz_configuration "$root"
        [[ $(qz_get "$QZ_CONFIG" version) == "$(qz_get "$QZ_BUNDLE/release.json" version)" ]] || qz_fail 'Use apply-update for an existing installation.'
        if [[ ! -e "$root/pending.json" ]]; then qz_verify "$QZ_CONFIG"; return; fi
        [[ $(qz_get "$root/pending.json" operation) == install ]] || qz_fail 'Retry the recorded update target.'
        file="$QZ_CONFIG"
    else
        for name in data releases current pending.json; do [[ ! -e "$root/$name" && ! -L "$root/$name" ]] || qz_fail "Existing state without its original installation.json: $root/$name"; done
        project="quazonai-$(id -u)-$(od -An -N6 -tx1 /dev/urandom | tr -d ' \n')"
        printf '{"root":%s,"uid":%s,"gid":%s,"port":%s,"project":%s,"bundle":%s}\n' \
          "$(qz_quote "$root")" "$(id -u)" "$(id -g)" "$QZ_PORT" "$(qz_quote "$project")" "$(qz_quote "$QZ_BUNDLE")" > "$QZ_WORK/new.json"
        file="$QZ_WORK/install.json"
        LC_ALL=C awk -v mode=merge -v path='' -f "$QZ_BUNDLE/json.awk" "$QZ_BUNDLE/release.json" "$QZ_WORK/new.json" > "$file"
        qz_save "$root/installation.json" < "$file"
        printf '{"operation":"install"}\n' | qz_save "$root/pending.json"
    fi
    qz_prepare "$root" "$file"; file=$QZ_PREPARED
    qz_save "$root/installation.json" < "$file"
    mkdir -p "$root/data"
    qz_compose "$file" run --rm --no-deps app configure "$root" "http://localhost:$(qz_get "$file" port)"
    qz_activate "$root" "$file"
}
qz_apply_update() {
    local root=$1 old candidate phase=preparing backup_policy=${QZ_BACKUP_POLICY:-required} saved_policy backup=null
    qz_configuration "$root"; old="$QZ_WORK/old.json"; cp "$QZ_CONFIG" "$old"
    qz_manifest "$QZ_BUNDLE/release.json"
    if [[ -e "$root/pending.json" ]]; then
        [[ $(qz_get "$root/pending.json" operation) == update && $(qz_get "$root/pending.json" target.version) == "$(qz_get "$QZ_BUNDLE/release.json" version)" ]] || qz_fail 'Retry the original pending target.'
        qz_raw "$root/pending.json" previous > "$old"
        phase=$(qz_get "$root/pending.json" phase)
        saved_policy=$(qz_get "$root/pending.json" backup_policy 2>/dev/null) || saved_policy=required
        saved_policy=${saved_policy:-required}
        [[ "$saved_policy" == "$backup_policy" ]] || qz_fail 'Retry with the original backup policy; --no-backup cannot change a pending update.'
    fi
    qz_prepare "$root" "$old"; candidate=$QZ_PREPARED
    if [[ ! -e "$root/pending.json" && $(qz_get "$old" version) == "$(qz_get "$candidate" version)" && $(qz_get "$old" local_runtime_service 2>/dev/null || true) == "$(qz_get "$candidate" local_runtime_service 2>/dev/null || true)" ]]; then qz_verify "$old"; return; fi
    [[ $(printf '%s\n%s\n' "$(qz_get "$old" version)" "$(qz_get "$candidate" version)" | sort -V | tail -n1) == "$(qz_get "$candidate" version)" ]] || qz_fail 'Use an explicit matching restore for a downgrade.'
    if [[ -e "$root/pending.json" ]]; then
        [[ $(qz_get "$root/pending.json" target.local_runtime_service 2>/dev/null || true) == "$(qz_get "$candidate" local_runtime_service 2>/dev/null || true)" ]] || qz_fail 'Retry with the original --local-runtime selection.'
    fi
    if [[ "$phase" == preparing ]]; then
        if [[ $(qz_get "$candidate" local_runtime_service 2>/dev/null || true) == nautilus ]]; then qz_runtime_stopped "$old"; fi
        printf '{"operation":"update","phase":"preparing","backup_policy":%s,"previous":%s,"target":%s}\n' "$(qz_quote "$backup_policy")" "$(qz_raw "$old")" "$(qz_raw "$candidate")" | qz_save "$root/pending.json"
        qz_stop "$old"
        # Refusal leaves the original app stopped, original DB untouched and the
        # original target recorded. Explicit start resumes only the old release.
        qz_idle "$old"
        if [[ "$backup_policy" == required ]]; then
            qz_backup "$old"; backup=$(qz_quote "$QZ_BACKUP")
        else
            printf 'Explicit --no-backup: preserving original data in place without creating a recovery point.\n'
        fi
        printf '{"operation":"update","phase":"starting","backup_policy":%s,"previous":%s,"target":%s,"backup":%s}\n' "$(qz_quote "$backup_policy")" "$(qz_raw "$old")" "$(qz_raw "$candidate")" "$backup" | qz_save "$root/pending.json"
    elif [[ "$phase" != starting ]]; then qz_fail 'Unknown pending update phase.'; fi
    qz_activate "$root" "$candidate"
}
qz_restore() {
    local root=$1 backup=$2 file="$2/installation.json" name destination
    qz_configuration "$root"; qz_manifest "$file"
    if [[ $(qz_get "$QZ_CONFIG" local_runtime_service 2>/dev/null || true) == nautilus || $(qz_get "$file" local_runtime_service 2>/dev/null || true) == nautilus ]]; then
        qz_runtime_stopped "$QZ_CONFIG"
    fi
    for name in root uid project; do [[ $(qz_get "$file" "$name") == "$(qz_get "$QZ_CONFIG" "$name")" ]] || qz_fail 'Restore only a matching recovery point at its original path.'; done
    [[ -f "$(qz_get "$file" bundle)/release.json" ]] || qz_fail 'Original release bundle must be retained for restore.'
    if [[ -f "$root/pending.json" ]]; then
        [[ $(qz_get "$root/pending.json" operation) == restore && $(qz_get "$root/pending.json" source) == "$backup" && $(qz_get "$root/pending.json" phase) == starting ]] || qz_fail 'Interrupted restore/update: preserve pending.json, data.restore and the recorded pre-restore backup; reconcile data replacement before resuming.'
        qz_raw "$root/pending.json" target > "$QZ_WORK/restored.json"
        qz_activate "$root" "$QZ_WORK/restored.json" --force-recreate
        return
    fi
    qz_stop "$QZ_CONFIG"; qz_idle "$QZ_CONFIG"; qz_backup "$QZ_CONFIG"
    destination="$QZ_BACKUP/data.original"
    [[ ! -e "$root/data.restore" ]] || qz_fail 'Preserve interrupted data.restore before retrying.'
    mkdir "$root/data.restore"
    if [[ -d "$backup/data" ]]; then
        cp -a --reflink=auto "$backup/data" "$root/data.restore/data"
    else
        tar -tzf "$backup/data.tar.gz" | awk '$0 !~ /^data(\/|$)/ || $0 ~ /(^|\/)\.\.(\/|$)/ {bad=1} END {exit bad}' || qz_fail 'Unexpected backup paths.'
        tar -xzf "$backup/data.tar.gz" -C "$root/data.restore" --no-same-owner
    fi
    printf '{"operation":"restore","phase":"restoring","source":%s,"target":%s,"backup":%s}\n' "$(qz_quote "$backup")" "$(qz_raw "$file")" "$(qz_quote "$QZ_BACKUP")" | qz_save "$root/pending.json"
    mv "$root/data" "$destination"; mv "$root/data.restore/data" "$root/data"; rmdir "$root/data.restore"
    if [[ -e "$backup/compose.override.yaml" ]]; then
        cp "$backup/compose.override.yaml" "$root/compose.override.yaml"
    else
        rm -f -- "$root/compose.override.yaml"
    fi
    qz_save "$root/installation.json" < "$file"
    printf '{"operation":"restore","phase":"starting","source":%s,"target":%s,"backup":%s}\n' "$(qz_quote "$backup")" "$(qz_raw "$file")" "$(qz_quote "$QZ_BACKUP")" | qz_save "$root/pending.json"
    qz_activate "$root" "$file" --force-recreate
}
qz_unpack() {
    local archive=$1 destination=$2 name
    [[ $(tar -tzf "$archive" | sort) == "$(printf '%s\n' "${QZ_BUNDLE_FILES[@]}" | sort)" ]] || qz_fail 'Unexpected deployment archive members.'
    mkdir "$destination"
    for name in "${QZ_BUNDLE_FILES[@]}"; do tar -xOzf "$archive" "$name" > "$destination/$name"; done
    qz_manifest "$destination/release.json"
}
qz_help() {
    printf '%s\n' 'Usage: bash manage.sh deploy|apply-update|update TAG|start|stop|restart|status|backup|restore PATH [--directory PATH] [--port PORT] [--no-backup] [--local-runtime]' \
      'Prebuilt Node/Docker deployment. Existing root, ports, config, credentials and original SQLite are retained.' \
      '--no-backup is valid only for update/apply-update; it never bypasses graceful stop or unresolved-task checks.' \
      '--local-runtime enrolls the existing stopped nautilus service for same-release image updates; its settings and data are preserved.' \
      'Only schema-5 installations are supported; prior formats are not migrated.'
}
qz_main() {
    local command=${1:---help} root="$HOME/.local/share/quazonai" argument= name
    shift || true
    if [[ "$command" == --help || "$command" == -h ]]; then qz_help; return; fi
    QZ_PORT=8081; QZ_BACKUP_POLICY=required; QZ_LOCAL_RUNTIME=false
    while (($#)); do
        case "$1" in
          --directory|--port) (($#>=2)) || qz_fail 'Missing option value.'; name=$1
            case "$name" in --directory) root=$2;; --port) QZ_PORT=$2;; esac; shift 2;;
          --no-backup) QZ_BACKUP_POLICY=skip; shift;;
          --local-runtime) QZ_LOCAL_RUNTIME=true; shift;;
          --help|-h) qz_help; return;;
          -*) qz_fail "Unknown option: $1";;
          *) [[ -z "$argument" ]] || qz_fail 'Unexpected argument.'; argument=$1; shift;;
        esac
    done
    [[ "$QZ_BACKUP_POLICY" == required || "$command" == update || "$command" == apply-update ]] || qz_fail '--no-backup is only valid for update/apply-update.'
    [[ "$QZ_LOCAL_RUNTIME" == false || "$command" == update || "$command" == apply-update ]] || qz_fail '--local-runtime is only valid for update/apply-update.'
    root=${root/#\~/$HOME}; root=$(realpath -m "$root")
    [[ "$root" != / && "$root" != "$HOME" ]] || qz_fail 'Select a dedicated installation directory.'
    [[ "$QZ_PORT" =~ ^[0-9]{1,5}$ ]] || qz_fail 'Select numeric ports.'
    QZ_PORT=$((10#$QZ_PORT))
    ((QZ_PORT>=1024 && QZ_PORT<=65535)) || qz_fail 'Select a port between 1024 and 65535.'
    # Legacy rejection precedes any Docker/systemd invocation or installation write.
    if [[ -e "$root/installation.json" ]]; then qz_configuration "$root"; fi
    qz_preflight; qz_lock "$root"
    QZ_WORK=$(mktemp -d "${TMPDIR:-/tmp}/quazonai-manage.XXXXXX")
    trap 'rm -rf -- "$QZ_WORK"' EXIT
    case "$command" in
      deploy) qz_deploy "$root";;
      apply-update) qz_apply_update "$root";;
      update) qz_configuration "$root"; qz_version "$argument"
        curl --fail --location --retry 3 --max-time 600 --output "$QZ_WORK/deploy.tar.gz" "https://github.com/zhengui666/QuaZonai/releases/download/$argument/quazonai-deploy.tar.gz"
        qz_unpack "$QZ_WORK/deploy.tar.gz" "$QZ_WORK/deployment"
        [[ $(qz_get "$QZ_WORK/deployment/release.json" version) == "$argument" ]] || qz_fail 'Downloaded version differs.'
        exec 9>&-
        local update_options=()
        if [[ "$QZ_BACKUP_POLICY" == skip ]]; then update_options+=(--no-backup); fi
        if [[ "$QZ_LOCAL_RUNTIME" == true ]]; then update_options+=(--local-runtime); fi
        bash "$QZ_WORK/deployment/manage.sh" apply-update --directory "$root" "${update_options[@]}";;
      start|restart) qz_configuration "$root"
        if [[ -e "$root/pending.json" ]]; then [[ $(qz_get "$root/pending.json" phase) == preparing ]] || qz_fail 'Retry the recorded installation/update target.'; fi
        if [[ "$command" == restart ]]; then qz_stop "$QZ_CONFIG"; fi
        qz_start "$QZ_CONFIG";;
      stop) qz_configuration "$root"; qz_stop "$QZ_CONFIG";;
      status) qz_configuration "$root"; printf 'Recorded %s: http://localhost:%s\n' "$(qz_get "$QZ_CONFIG" version)" "$(qz_get "$QZ_CONFIG" port)"; qz_compose "$QZ_CONFIG" ps;;
      backup) qz_configuration "$root"; qz_stop "$QZ_CONFIG"; qz_idle "$QZ_CONFIG"; qz_backup "$QZ_CONFIG"; printf 'App remains stopped. Use start when ready.\n';;
      restore) [[ -n "$argument" ]] || qz_fail 'restore requires the original recovery-point directory.'; qz_restore "$root" "$(realpath -e "$argument")";;
      runtime|recover-access|apply-config|source) qz_fail 'Retired Rust transport command. Configure datasets/connections in data/service.json and use the Node CLI.';;
      *) qz_fail "Unknown command: $command";;
    esac
}
if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then qz_main "$@"; fi
