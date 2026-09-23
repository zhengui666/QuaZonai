# 原生 Runtime 与 job 镜像

portfolio-study/6及portfolio-history/1支持STUDY_PORTFOLIO：原模型/校准与费用字节绑定，固定截止
历史前缀驱动Wasm/Clarabel，在单个Nautilus账户按实际权益/权重调仓。
输出原源质量和逐帧原输入/求解/模拟报告；不可行不输出模拟结果。
另输出原manifest绑定的qz.portfolio_history/1 Arrow IPC文件。contracts中的薄
Arrow合同共同用于job写入/回读与采纳；全部行/列/元数据必须匹配原请求与报告。
portfolio-rolling-liquidity/1从原PARAMETERS政策和每截止目录前缀测量BAR估值，
复用DATA_VALIDATE的原生估值，按实际模拟权益和参与率约束调仓并再次检查年龄。
portfolio-build-rolling/1将同一原政策/原生测量用于单截止Build，报告保留bar_notionals，
结果核对原资产、时点、币种、年龄与精确名义量；Store 已在 Build 准入、Candidate 发布及 Release 冻结时重验原滚动来源。
不复用过期单次快照，不声称真实深度。支持固定间隔或原参数manual_cutoffs_ns手动截止，
共用原帧数/fuel/范围/TTL覆盖检查。portfolio-calendar/2另绑定原完整会话PARAMETERS
文件，逐值匹配目录原元数据登记的完整会话表，截止取原收盘加偏移；不自造节假日规则或抓取URL。
原文件解析成功不是完整来源准入、正式PORTFOLIO发布或PASS。
本机实际链验证：`cargo test --locked -p job --test study`。

portfolio-sequence/1支持SIMULATE_PORTFOLIO_SEQUENCE：原Candidate目标文件序列、
可信可用时间和原费用文件逐项绑定，再进入同一个原生账户。完整源质量与实际
模拟窗口分别报告。不是正式PORTFOLIO评估发布；完整序列准入及政策仍待接入。

candidate-simulation/2支持SIMULATE_CANDIDATE：重读原Candidate目标和执行设置，
复用原生DATA_VALIDATE检查原source_selection并输出qz.data_quality；保持模拟
另输出qz.native_simulation，两个窗口不混算缺失比例。
核对唯一目标及其因果有效区间，再调用既有共享资金模拟。只允许唯一FORWARD目录
及原目标REPORT/费用PARAMETERS；不读取真实账户，不创建Evaluation或交付资格。

portfolio-cost-source/1要求Build挂载原transaction_costs_ref的PARAMETERS字节，
与冻结execution_settings完整一致；发布再次核对保存配置。
不把费用副本或模型参数当DATA_BACKED证明。
Build与模拟共用原生市场/费用检查：Forward实际Instrument的币种和maker/taker
须匹配同一执行设置；不能等模拟才发现Build已用错费率。
portfolio-slippage/1从原最后BAR及Instrument tick产生slippage_references，按
DESIGN A5.2换算参考价下的模型期望规划费率，向上舍入18位。零概率引用列表为空；
这不是未来费用上界、盘口冲击或DATA_BACKED；实际Nautilus费用不被二次扣除。

portfolio-liquidity/1消费冻结的原DATA_QUALITY报告，核对原Dataset/选择、币种、
年龄与资产量，再应用已有参与率约束；不是实时深度或DATA_BACKED证明。
Store仍负责原来源采纳、许可和发布期限，不能直接提交原生任务冒充资格。

bar-notional/1通过原生DATA_VALIDATE输出非Sealed最后已知BAR的价格、成交量和
Instrument::try_calculate_notional_value收盘估值，保持原币种与时间。没有自研合约
估值公式，不将历史观察冒充未来流动性或完整成本资格；Sealed明细保持不输出。

本目录维护 `apps/runtime` 的原生镜像装配入口，不包含开发用 Codex 执行器、模型账号、数据库、应用密钥或整个工作区。产品字段与验收边界以 `DESIGN.md` B4 为准。

Runtime 是受信任的计算网关：只接受已登记镜像、不可变输入引用和固定类型任务，复用 Docker 的进程、文件系统与 cgroup 隔离。研究预算、数据许可、Alpha 资格、审批、真实交易仍不由 Runtime 拥有。任务成功不等于科学结论通过。

当前PORTFOLIO_BUILD要求原FORWARD目录和MODEL/校准产物，经原生预测与收益
生成、ndarray聚合/样本估计后进入同一Clarabel问题；不再接受手填预测数组。
资产groups由Store从原Forward Universe决策时有效且可用的唯一成员记录冻结，
沿既有组约束进入求解与原结果绑定；Runtime不猜分类，不提升原数据资格。
真实OCI回归须覆盖原参数/模型上传、目录挂载、原结果下载/绑定及幂等重放。
合成数值不是REAL资格或完整交付。更新源码后必须重建并登记新镜像。
方差上限还要求portfolio-variance-bound/1，使用原生Cholesky/Clarabel二阶锥；
上限是每决策周期收益方差，发布按冻结历史与保存权重重新计算，不是年化波动率。
CVAR还要求portfolio-cvar/1及LINEAR_PROGRAM，显式冻结cvar_confidence为(0,1)
内Decimal。Clarabel原生LP消费全部等权损失场景；此时上限是同周期预期损失
收益率，发布含分数尾部/重复损失复核，不把原生数值结果当资格。
方差风险预算需portfolio-risk-budget/1与SECOND_ORDER_CONE，冻结risk_budgeting的
资产份额/方向/总敞口。原生二次锥问题求预算后，再由原组合问题核对全部约束与费用；
迭代预算合计，不能为了满足另一约束改变预算比例。
CVaR预算另需portfolio-cvar-risk-budget/1与POWER_CONE（及原CVAR能力），原生
加权几何平均/场景LP保留对偶见证。发布复核原尾部概率约束、正CVaR及各贡献，
不挑选并列场景、不归一化对偶或改动预算。无界/零风险/失败不返回目标。
当前要求portfolio-models/4，绑定原optimizer/alpha_ensemble的类名、版本及严格
参数；risk_aversion冻结在optimizer.parameters，不接收顶层settings/risk_aversion
或默认模型。VARIANCE从目录生成的return_history经covariance_estimator原生估计协方差，
CVAR直接使用完整原收益场景，不以协方差或正态分布替代；
不接受旧covariance矩阵。具体原生请求格式以CLI及生成合同为准。

## 构建

受管组合要求portfolio-weights/1，显式current_weights_artifact_id及current_weights，
以REPORT挂载原权重JSON；原生job读取并逐字段核对，不从资产权重猜测来源。
来源标签只是冻结计算输入，Store还须核验下游快照或原Candidate及当前资格。

原生模拟要求simulation-models/1，显式fee_model/fill_model/latency_model引用
进入Nautilus0.63.0配置；滑点及固定随机种子由DefaultFillModel执行，费用由
MakerTakerFeeModel执行，延迟由StaticLatencyModel执行。旧单项insert_latency_ns
请求不兼容，不在运行时替调用者补模型或种子。

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

装配同时保留 `ldd` 报告的依赖实际路径与绝对请求路径，包括 usr-merged 主机的 `/lib64` 动态加载器；镜像不依赖宿主目录软链。仅在新镜像根内将选定的公开原生文件设为可读、可执行文件设为可执行，不改变宿主工具链权限。构建后的真实 `job --version` 或 `rustc --version` 失败即构建失败。

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

<a id="recovery"></a>
## 冷备份与恢复

暂停控制面新任务与 Worker，先核对实际远端任务终态，再停止唯一 Runtime 写入者。备份整个原生状态目录，包括 SQLite/WAL、对象、manifest、实例身份和取消记录；同时保留控制库、私有产物、master key、Runtime 配置、原镜像与原生数据目录。状态目录示例按实际安装替换，备份父目录须预先存在。

```sh
set -eu
umask 077
state=/srv/quazonai-runtime/state
backup_parent=/srv/quazonai-runtime/backups
test -d "$state" && test ! -L "$state"
test -d "$backup_parent" && test ! -L "$backup_parent"
checkpoint=$(mktemp -d "$backup_parent/runtime-XXXXXXXX")
: > "$checkpoint/state.tar"
sudo tar --create --numeric-owner --file "$checkpoint/state.tar" --directory "$state" .
sudo tar --compare --numeric-owner --file "$checkpoint/state.tar" --directory "$state"
```

检查并明确选择恢复点，不自动选择最新目录。保留原目录，以原路径恢复整个 checkpoint；不能混合多个备份中的 SQLite/WAL/对象。以下两个 REPLACE_WITH 值必须先替换为已核对的目录，retained 使用未占用的同级目录。

```sh
set -eu
umask 077
state=/srv/quazonai-runtime/state
checkpoint=/srv/quazonai-runtime/backups/REPLACE_WITH_SELECTED_CHECKPOINT
retained=/srv/quazonai-runtime/REPLACE_WITH_UNUSED_RETAINED_DIRECTORY
test -s "$checkpoint/state.tar"
test -d "$state" && test ! -L "$state"
test ! -e "$retained" && test ! -L "$retained"
mv --no-target-directory --no-clobber -- "$state" "$retained"
mkdir -m 0700 -- "$state"
sudo tar --extract --numeric-owner --same-owner --preserve-permissions \
  --file "$checkpoint/state.tar" --directory "$state"
sudo tar --compare --numeric-owner --file "$checkpoint/state.tar" --directory "$state"
```

恢复与原副本不得同时启动。使用原用户、配置及路径启动 Runtime，重读原任务的身份、状态、时间、manifest 和输出字节，验证取消记录与同键重放保持一致；缺镜像、数据或输出时明确报错。Worker 用原 Attempt/外部 ID 对账，不能重启原容器冒充恢复。确认原记录后，执行一个有界新任务并读取原数据，随后恢复正常任务接纳。原目录及 checkpoint 按现有保留策略处理。

原生冷恢复与控制面联合恢复测试分别为 [native_restore](../../apps/runtime/tests/native_restore.rs) 和 [native_control_restore](../../apps/runtime/tests/native_control_restore.rs)，由 [Native Runtime CI](../../.github/workflows/native-runtime.yml) 使用真实镜像、数据库和 Worker 运行。测试使用独立临时资源，不能传入生产数据库。
