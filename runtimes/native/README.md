# 原生 Runtime 与 job 镜像

本目录维护 `apps/runtime` 的原生镜像装配入口，不包含开发用 Codex 执行器、模型账号、数据库、应用密钥或整个工作区。产品字段与验收边界以 `DESIGN.md` B4 为准。

Runtime 是受信任的计算网关：只接受已登记镜像、不可变输入引用和固定类型任务，复用 Docker 的进程、文件系统与 cgroup 隔离。研究预算、数据许可、Alpha 资格、审批、真实交易仍不由 Runtime 拥有。任务成功不等于科学结论通过。

## 构建

支持 Linux x86_64、原生 Docker Engine、cgroup v2、Rust 1.98.1。构建机需有该 Rust 工具链、`wasm32-unknown-unknown` 标准目标、GNU coreutils 的 `timeout`、`ldd` 和 Node.js。Docker socket 由操作者正常授权；不要给公共 HTTP 或研究 Agent 暴露 socket，也不要以放开 socket 为匿名写入来解决权限错误。

先从本次要部署的源码构建，再装配新的镜像目录。构建入口不会替用户选择依赖版本或执行 `cargo update`。

```sh
rustup toolchain install 1.98.1 --profile minimal --target wasm32-unknown-unknown
rustup run 1.98.1 cargo build --locked --release -p job -p runtime
assembly_parent=$(mktemp -d)
node runtimes/native/build-native-image.mjs --profile release --output-dir "$assembly_parent/image"
```

装配结果的 `image-id.txt` 是 **Docker 返回的原生完整 image ID**，形式为 `sha256:<64个小写十六进制字符>`。将这个精确值登记在 Runtime 配置中，不使用 `latest` 或可变 tag。跨宿主传输使用原生 `docker image save/load` 或登记仓库的原生 digest，不以应用自行计算的 hash 代替镜像身份。

镜像使用 `FROM scratch`，只复制已构建的 `job`、选定 rustup 的 rustc/标准库/原生 linker、实际 ELF 所需共享库和 GNU timeout；装配记录保留原生命令、版本、Cargo.lock 与可取得的发行许可说明。构建上下文与诊断日志目录分开，不发送工作区、模型 profile、密钥或数据库。镜像 ID 是这次选定原生文件的身份，**不是跨发行版主机逐字节可重复构建的声明**。

`--isolation-probe` 只用于明确的 CI 测试镜像，正式部署不传该参数。镜像中的 `job --version` 和 Runtime 的可用性检查仍不能替代真正执行、取消及崩溃恢复验收。

## Runtime 配置

配置是受信任的本机 JSON 文件，通过 `runtime serve --config /absolute/runtime.json` 使用；HTTP 请求不能修改下面的宿主路径、镜像、命令、挂载或环境。

```json
{
  "schema_version": 1,
  "state_dir": "/srv/quazonai-runtime/state",
  "credential_file": "/srv/quazonai-runtime/runtime-credential",
  "docker_socket": "/var/run/docker.sock",
  "bind": "127.0.0.1:8790",
  "images": [
    {"job_kind": "DATA_VALIDATE", "image_ref": "REPLACE_WITH_NATIVE_IMAGE_ID"},
    {"job_kind": "ALPHA_EVALUATE", "image_ref": "REPLACE_WITH_NATIVE_IMAGE_ID"},
    {"job_kind": "PORTFOLIO_BUILD", "image_ref": "REPLACE_WITH_NATIVE_IMAGE_ID"},
    {"job_kind": "PORTFOLIO_SIMULATE", "image_ref": "REPLACE_WITH_NATIVE_IMAGE_ID"}
  ],
  "catalogs": [],
  "max_cpu": 2,
  "max_memory_mib": 4096,
  "max_wall_seconds": 3600,
  "max_output_bytes": 67108864,
  "max_parallel_jobs": 2,
  "max_pending_jobs": 64,
  "storage_quota_bytes": 10737418240
}
```

`REPLACE_WITH_NATIVE_IMAGE_ID` 必须替换为装配实际返回值；它不是可运行默认镜像。`state_dir` 的父目录需由运行用户管理，状态目录为0700。凭据文件为该用户可读的0600普通文件，内容是32–8192字节非空白可打印ASCII，可有一个末尾换行。凭据由操作者在网关与控制面分别通过正式配置绑定，不能出现在URL、命令参数、Git、Issue或日志里。

`catalogs` 中每项为 `{ "root": "/absolute/immutable/catalog", "metadata_file": "/absolute/catalog-metadata.json" }`。元数据严格使用生成的 `RuntimeCatalogMetadataV1`，完整记录原生snapshot/version、partition、native instrument definitions、历史成员、原始质量报告及可用时间来源。空登记列表不会自动制造数据、PIT验证或可交易市场。不得把测试数据的FIXTURE/SYNTHETIC标记改为REAL，也不能仅凭 `ts_init <= cutoff` 宣称历史可用性已验证。

数据目录不得包含状态目录、凭据、Docker socket或其他登记根。每次任务只挂自己的精确InputSet对应目录；Rust编译任务不获得任何Dataset挂载。配置变更后新的任务需重新探测能力；已有任务继续使用其持久化的原始launch和稳定远端身份，不能把新路径或新参数替换进旧任务。

```sh
target/release/runtime doctor --config /absolute/runtime.json
target/release/runtime serve --config /absolute/runtime.json
```

`doctor` 使用真实 Docker API、cgroup能力与镜像检查，缺Docker权限、镜像、协议或资源能力即失败；不会写业务数据库。`serve` 可以在Docker临时不可用时继续提供已有任务状态、幂等重放和取消tombstone，实际新任务仍不可假报成功。

## TLS 与入口

Runtime HTTP只监听显式loopback地址。跨宿主访问通过同宿主、已有受信任TLS反向代理转发到该监听端口。控制面登记的是HTTPS origin、真实CA策略和允许SocketAddr；它仍验证Host/SNI与证书，不因内部loopback转发关闭TLS。

仅在操作者显式选择本机开发HTTP时，控制面才允许literal-loopback HTTP。没有公共HTTP的默认回退。Runtime credential只经唯一Bearer头传输；网关不会将它传给job或编译器。

## 状态、取消与恢复

网关状态由SQLite WAL/FULL事务保存，数据库只拥有原生任务身份、精确输入字节和输出/manifest，不拥有研究预算或资格。相同external_job_id和原始JobSpec重放同任务，不同请求409。未知身份的取消先建立永久tombstone，迟到提交不再执行。

CREATE/START意图先持久化再调用原生Docker。START结果未知时只查询同一container ID，不重新启动已退出容器。取消已可能发送的任务时，确认停止并移除旧ID、以同一个原生名称建立永不启动的屏障容器，之后才封口取消；晚到CREATE冲突，旧ID START失败。普通404、请求超时或Gateway退出都不是取消确认。

`job run-bounded` 在实际启动时读取冻结deadline并交由镜像内GNU timeout约束，Docker init和cgroup管理子进程。Gateway退出不取消这项原生墙钟限制；Gateway重新启动后仍采纳原身份的真实终态。未取得精确最终CPU或内存峰值时返回null，不能把早先采样或0伪装成最终计量。

不要手工删除journal、输入对象、正式输出或terminal tombstone来“修复”任务。对状态目录备份/恢复必须停止写入并保持整个SQLite状态；恢复后的原生Docker身份也要可查询，不能只恢复一个JSON文件后重新跑全部任务。

## 验证

普通 `cargo test --locked -p runtime` 执行真实SQLite事务、HTTP和生成合同回归；它不构成原生OCI隔离证据。原生OCI套件由独立必跑CI明确启用：

```sh
QUAZONAI_NATIVE_JOB_IMAGE='sha256:ACTUAL_NATIVE_IMAGE_ID' \
QUAZONAI_DOCKER_SOCKET=/var/run/docker.sock \
rustup run 1.98.1 cargo test --locked -p runtime --features native-oci --test native_oci -- --test-threads=1
```

该套件需要含CI专用isolation-probe的实际构建镜像。缺镜像、Docker或cgroup前提会失败，没有“缺环境则跳过”的成功分支。它验证真实Rust→Wasm编译、唯一提交与不可变结果、Gateway崩溃后的同身份恢复、脱离Gateway的墙钟限制、取消的晚到CREATE/START屏障，以及真实UID/网络/文件/内存/PID限制。控制面的研究→独立评估→多Alpha→目标交付与全部T01–T42仍必须另外完成；不能用这份Runtime验收替代整个Issue62。
