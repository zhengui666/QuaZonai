# 运行与部署

本分支已实现 Rust 原生组件、逐轮 PostgreSQL Store 和可运行的浏览器认证 API，**尚非完整研究与交付产品**。旧实现已删除，无兼容服务；完整目标和完成条件在 DESIGN。

## 原生计算 Runtime 的独立运行边界

`apps/runtime` 的配置、实际原生镜像装配和启动说明集中在 [runtimes/native/README.md](runtimes/native/README.md)，由 `runtime doctor/serve --config` 读取受信任本机文件。网关独占自己的0700状态目录与SQLite日志，以原生OS文件锁防止两个监督者同时使用同一目录。它使用操作者正常授权的Docker Unix socket；无权访问时明确不可用，不修改sudo、用户组、socket权限或改用无隔离执行。

已发送计算与网关进程分开：退出网关不表示任务停止，固定job入口中的GNU timeout仍约束该次原生墙钟。恢复必须保留原始JobSpec、native container ID、发送意图与取消tombstone，查询同一身份；不得清空journal、改ID或重新START已退出容器来伪造恢复。旧原生镜像/输入缺失、Docker不可用或提交结果未知时保留不可用/待对账事实。数据和资格的正式采用仍由控制面独立决定。

运行环境探测本身也使用严格发布语义。失败的探测快照仅在重新取得原Operator事务锁、从主库确认精确原生对象没有任何Artifact引用之后才回收；已提交、结果未知但可能被引用、或无法核对的对象都保留。回收失败不会把原请求变成成功；日志只有无秘密的Artifact ID与静态原因，不能用手工删除所有产物来替代核验。

物化配额分别计算原始SQLite对象、将来正式输出和执行目录中的输入/输出副本，不把同一份字节的第二个落盘位置忽略。终态提交后监督者只回收这个任务的派生目录，先移动到已预约的私有临时槽再删除、同步目录，最后释放物化配额；原始对象、正式输出、manifest和原生任务/取消身份继续保留。重启会先核对并计入历史物化目录，无法证明归属的目录保留并报错，不通过全目录清空“修复”。存储额度不足是明确阻塞，不能减小已预约字节或假称任务执行完成。

取消发生之前已经观察到的原生非零退出、OOM或超时保留原始失败原因及完成时间，即使删除旧容器、安装取消屏障之间重启也不改成取消成功。取消之后的信号退出不会倒推成更早的研究失败。Runtime OpenAPI对全部路由明确声明实际Bearer认证及401，不把内网接口描述成匿名API。

## 首次启动认证服务

### Codex 原生目录、模型与账号

服务启动参数 `serve --codex-deployment /absolute/path/codex-deployment.json`（或
`CODEX_DEPLOYMENT`）指定部署所有者的JSON文件；不设置时账号目录列表为空，不使用宿主默认目录。
文件结构如下，所有路径须由部署者替换为已经存在的绝对路径；binary必须是锁定的官方0.144.4：

```json
{
  "schema_version": 1,
  "binary": "/opt/codex/bin/codex",
  "executable_path": "/usr/local/bin:/usr/bin:/bin",
  "bindings": [{
    "reference": "research-native",
    "label": "研究专用原生账号目录",
    "profile_origin": "MANAGED_VOLUME",
    "home": "/var/lib/quazonai/codex-home",
    "codex_home": "/var/lib/quazonai/codex-home",
    "working_directory": "/var/lib/quazonai/codex-workspace",
    "environment_names": []
  }]
}
```

已有目录的显式挂载使用`OPERATOR_MOUNT`。每个CODEX_HOME只绑定一个标签；API不会创建、复制或删除认证目录。

研究Mission要求专用profile，不带个人`AGENTS.md`/`AGENTS.override.md`或`instructions`/`developer_instructions`/`model_instructions_file`覆盖。锁定原生版本没有关闭全局个人提示注入的stdio开关；遇到这些配置会在模型请求前拒绝，不能把“已登录/模型可用”当作Mission已经就绪。请使用独立命名卷或显式的专用挂载，按原生流程登录；系统不读取或删除个人提示、不复制auth.json，也不暗换账号/profile。
只有`environment_names`明确列出的服务环境变量会传给该原生进程，JSON不写凭据值；不要传数据库、钱包、Broker或无关秘密。
该账号操作所有者使用单个API进程；不能让多个API或外部登录进程同时管理同一CODEX_HOME。

在“设置 → Codex 模型与连接”登记Profile，选择上述标签。SYSTEM沿用原生配置与认证；
CUSTOM_PROVIDER使用独立写入的Provider凭据，不能从账号登录入口更改系统订阅。
“登录ChatGPT账号”确认后在Codex返回的验证页输入设备码；不要把设备码、密码、Token或auth.json发给模型。
设备码只留当前网页内存及有界原生所有者，刷新后只能读操作状态；同一发起页可用原请求重新显示。
Codex自行完成OAuth并保存/刷新令牌，QZ不实现另一套OAuth流程。

取消按钮仅请求取消，直到实际状态显示取消已确认才算取消。登录成功与取消竞争时保留原生完成结果。
网页关闭不取消已接受的操作；服务重启、超时和UNKNOWN不能证明账号未变化。
登录或注销开始后旧模型观测失效，操作结束后点击“探测Codex连接与模型”；探测不发起付费推理。
模型与推理Slider只使用这次有效的原生目录，默认设置不发送覆盖，不改变已有Thread或研究预算。
本地协议、数据库和浏览器测试不替代受保护的真实账号登录及推理验收。

### 应用认证与数据库

依赖固定 Rust 工具链及 PostgreSQL18 + PGMQ1.10.0，使用独立的新数据库。由原生 PostgreSQL 管理工具创建不带超级用户、创建数据库、创建角色权限的应用登录角色，密码通过交互或受保护配置输入；迁移身份与应用身份分开。

CLI.md 中 `init-state → migrate → bootstrap → serve` 是实际可执行入口。`migrate --application-role NAME` 通过 SQLx 和 tower-sessions 原生迁移创建域表及会话存储，授权应用 DML；`serve` 不执行迁移，并拒绝高权限/owner 数据库连接。升级前暂停 HTTP/CLI/MCP 写命令和 Worker，并等待旧事务结束；只用 `cargo run --locked -p server -- migrate`，不要在活跃库上直接执行 SQLx CLI 或单条迁移 SQL。该命令先用原生迁移锁和应用表写冲突锁保护整个待应用批次，失败全部回滚；锁超时应排查旧事务后重试，不杀事务或放宽锁跳过验证。0006 安全升级会撤销已初始化实例的全部历史浏览器/设备和一次性 Operator 授权，须重新 TOTP 登录；旧审计记录保留。生产入口使用同源 HTTPS，监听内部地址并由受信任反向代理终止 TLS、保留 Host；不要将明文内部端口直接暴露公网。

`bootstrap` 只在本机显示一次 `capability_id/capability/expires_at`。浏览器使用该凭据请求 `POST /api/v2/bootstrap/start`，获得只展示一次的原生 `otpauth://` URI；扫码后提交 `/bootstrap/confirm` 的六位动态码。初始化确认与首个登录权限在同一事务提交，完成后所有 bootstrap capability 失效。

正常登录仅提交 TOTP，勾选信任设备时同时提供标签。普通会话12小时，信任设备30天；到期不延长。会话 cookie 由 tower-sessions 原生私有 cookie 管理，HTTP-only、SameSite Strict、根路径、生产 Secure。API 无需/不接受浏览器提交用户名、密码或 cookie 内的自报权限。

## 撤销、重放与故障

每次请求通过 PostgreSQL 的登录权限、设备状态和认证 epoch 复核，不只相信 cookie。注销先提交数据库撤销再删除原生 Session；并发请求保存旧 Session 也不能恢复登录。删除信任设备需最近300秒内 TOTP 验证。动态码按实际匹配的时间步一次性消费，±1步容差不允许重放；全局每操作60秒最多5次验证，多个 API 实例共享数据库限流。

业务、认证响应均 `Cache-Control: no-store`；浏览器写入必须携带与 PUBLIC_URL 完全匹配的 Origin。数据库、Secret Store 或 Session Store 不可用时拒绝操作，不能退回匿名或内存认证。失败响应只包含安全错误和请求编号，不含路径、密钥或 SQL 详情。

## 机器凭据和项目管理

项目与身份 API 的实际路径和严格 DTO 由 `cargo run --locked -p server -- openapi` 导出。浏览器登录后使用 `POST /api/v2/projects` 创建项目，`PATCH /api/v2/projects/{id}` 必须带当前 `expected_revision`；所有管理写请求必须提供非空且不超过200字节的 `Idempotency-Key`。重复同键/同请求只返回已提交的原始响应，不把后来修改过的对象冒充首次结果；同键不同内容返回409。项目未绑定已冻结 Brief 不能激活，归档后不能原地复活。

Operator 可创建独立 CLI/AUTOMATION/DOWNSTREAM 主体，系统任务的 MISSION 身份不由公共 API 创建。每个凭据只在首次响应中显示完整 `qz2.<public_id>.<opaque>` token；数据库只登记不可逆原生 verifier 的 SecretVault 引用，列表、回执、日志不含秘密。准确重试签发返回同一凭据和 `token:null`，不是再显示秘密；首次响应丢失时撤销该凭据并以新键重新签发。不要在 URL、命令行参数、issue、Agent prompt 或浏览器持久缓存中放 token。

机器请求只能在 `Authorization: Bearer ...` 中提交一次，不能同时附带浏览器 Cookie。机器读写在业务事务内再次检查 scope、精确 project/run/downstream、到期、撤销和主体 epoch。禁用/重新启用主体都推进 epoch；旧凭据不复活。DOCTOR_READ 是独立只读 CLI/AUTOMATION 权限，不能与其他权限混合、不能授给 Downstream/Mission。

人工 CLI 需要管理操作时，通过 `/auth/operator-command-grants` 提交真实 TOTP、封闭 operation 和完整预期请求，取得最长300秒且一次性的 grant；操作时用 `X-Operator-Grant`。创建资源的 UUID 由服务器选定，已存在资源必须指定精确 target。该授权不改变机器身份、不向 Agent 授予 Operator 权限，AUTOMATION/MISSION/DOWNSTREAM 不能领取。撤销、过期、请求替换、目标替换和再次使用不同键都拒绝；已提交的完全相同重试仅能读原回执。读取回执仍要求当前有效的机器凭据和相同认证 epoch，但先于新的 TOTP 校验与 REAUTH 限流，因此旧动态码过期或新验证额度用尽不会把已提交授权误报为失败；原授权到期时间不延长。

机器 capability 的原生 Argon2 校验前，PostgreSQL 原子预约60秒窗口：每凭据最多5个、全局最多32个失败或在途尝试。成功仅归还所属原窗口的占用，失败、取消和计算槽繁忙保留至窗口重置；429响应含 Retry-After。机器计算使用独立2个槽，不占用浏览器 TOTP 的2个槽；多个实例共享数据库窗口。不要以增加实例绕过限流。

## 正式数据登记与集成管理

浏览器“数据”页提供数据源、许可、原生版本、Universe 的管理；“设置”的计算端和下游页登记集成、原生凭据引用及计算端探测。所有页面使用同一 Rust 生成接口合同，移动端不更换权限或执行规则；离线不发送写入，连接中断后保持原请求的幂等键，不能因为失败提示就认为服务器没有提交。凭据只进入当次 write-only 请求，登记成功立即清空输入，不进入地址、浏览器持久存储或配置正文。

先在真实 Runtime 的配置中登记已拥有许可的不可变 Nautilus 目录及其原生 metadata 文件，再在控制面创建 Source，填写该 Runtime 与精确 registry key。registry key 不是宿主目录或 URL，控制面不自动抓取任意来源。原生 metadata 保留 native_snapshot_ref、storage_version、分区、实际 provenance、PIT 说明、质量及 Universe；正确的时间排序或字段格式并不是历史 PIT 的证明。测试/合成来源不会升级为 REAL。

许可需明确原始授权说明、有效期和用途，并引用由 Operator 提交的非空 REPORT 证据。撤销追加记录，不删除或修改过去的许可；省略生效时间表示数据库当前时间，显式时间只允许未来生效。读到 ACTIVE 是该次查询时的状态，不是永久权限或免于下一次检查。全局数据管理只读机器入口仅接受独立 DOCTOR_READ 的 CLI；Mission/Downstream 不因拥有项目读权限取得该管理入口。

登记 Dataset 时只提交 Source、精确许可、Source/Runtime 当前 revision、原生存储版本及可选已有 Universe。服务经已配置的真实 TLS Runtime 读取 metadata，不接受客户端自报的 origin、PIT、计数、质量或原生报告。同一 Source/native_snapshot_ref/storage_version 只能保留一个身份；改变许可、分区或原始内容会冲突，不能换 UUID 重置 Sealed 暴露。已有 Universe 只有完整原生定义一致才可复用。登记成功只表示来源引用和授权证据已原子保存，不等于已执行 DATA_VALIDATE、独立评估或产生合格 Alpha。

`024_data_registration` 增量迁移新增不可变 dataset_registration_evidence，不为旧 Dataset 推断或补造原生证据。升级仍使用停写、备份及正式 `server migrate` 入口；旧表、历史来源和授权保留。网络、文件发表或事务失败时精确回收未被引用的本次对象，不扫描其他产物；数据库结果未知时保留对象并给出错误，不误删可能已提交的证据。

用户命令已接入原生 `server client`，具体命令、单次 TOTP grant、stdin JSON、私有凭据文件、SSE cursor 与导出退出码见 CLI.md。它只经 HTTP 使用现有权限，不能通过直接 SQL、应用 Master Key 或读 Vault 绕过同一 API。机器管理授权请求正文一旦改变，即使使用原 key，也可能先被单次 grant 的完整意图约束拒绝为403；只有当前授权通过后才进入回执冲突检查。正确重放必须保留原命令、正文、目标和幂等键。

## Worker、正式数据验证与025升级

`server worker` 使用与API相同的非owner应用账号、私有状态卷和受信任的Runtime目标配置，作为独立常驻进程启动。它读取现有PGMQ任务，不在控制面运行Nautilus、编译研究模型或维护另一份队列；默认科学任务和已启用的Mission各最多2个在途驱动，`WORKER_PARALLELISM` 可设1–32并分别应用于两类容量。SIGINT/SIGTERM停止领取新任务并排空已开始的有界I/O，不把停止Worker等同于停止远端计算。

迁移 `202609110025_native_tasks.sql` 新增三个不可变原生关联表：`run_native_tasks` 保存和准入同事务冻结的任务定义，`run_native_attempts` 保存唯一首次派发的JobSpec，`run_native_outputs` 记录远端原生对象与本地生产者产物的精确映射。旧Run不回填或假造这些定义；部署停写并使用正式 `server migrate`，保留旧历史，不直接修改迁移记录或队列表。

`POST /api/v2/data/validate` / `server client data validate` 为同项目已冻结且已正式登记的DISCOVERY/VALIDATION输入排队，严格字段及人工grant流程见CLI。该入口不能读SEALED或接受任意命令、URL、镜像和客户端报告；202不是计算成功，必须继续查看真实Run。累计CPU秒、墙钟、内存和输出均受请求及原生Runtime上限约束，不计入科学试验但不获得无限资源。

Worker可使用当前Run租约刷新到期探测。一次提交应答丢失后，只查询原任务ID；租约接管保持原Attempt和JobSpec，拒绝旧owner。404、网络断开、退出Worker均不允许发布取消成功或提前释放任务；只有匹配身份的原生持久终态可完成对账。终态原始manifest、全部允许的原生产物、生产者关系、事件与唯一回执一起提交后才archive。取消先提交时，迟到的成功payload不发布；格式错误的已完成输出保留失败审计，不能升格为Alpha资格。

029迁移增加不可变Mission归属和非秘密Profile快照。正式Cycle的首个原生DATA_VALIDATE完成后，Worker在确认消息前原子创建研究Mission；重复通知不会创建第二份。暂停保留恢复通知，进行中的原生账号操作显示WAITING_FOR_CODEX_ACCOUNT并等待；Profile或数据许可需人工处理时Cycle显示WAITING_INPUT，不会暗换账号。CPU预算耗尽如实结束周期，没有Mission半状态或额外试验额度。

030迁移为正式提案与原生CompileModel保存不可变关联，开始编译后不能替换原提案的
代码/参数指针；修正应提交带原父血缘的新提案。它不回填历史作者或运行结果，也不
新增Agent任意执行命令。编译仍是有预算的DATA_VALIDATE，不读取市场分区，不新增
科学试验；MODEL只可来自原生生产者，不能把编译成功当成Alpha资格。可信Store已
编写原子准备入口，不能手工补库宣称任务完成。

031迁移增加提案到Discovery预测Run的不可变关联。可信准备入口只使用原提案编译
成功后采纳的MODEL和参数明确选择的冻结Discovery版本，预约一次科学试验；重放
保留同一Run，不再扣额度。PARAMETERS格式见CLI，当前只支持Brief的FIXED_BARS
horizon。启用Mission的Worker在最新Turn结算后，每次消费按ordinal准备一个就绪的
编译/预测步骤，复用原Mission JobLimits及当前累计预算；科学Worker独立执行。
每步前以当前Mission租约刷新已过期/临近过期的原Runtime能力探测，复用已登记
目标与Vault引用；原生HTTP在数据库事务外执行，60秒有效期不因模型耗时而延长。
原生调用未知或仅有部分usage时不开始这些步骤；已排队的任务不重建。预测成功
仍不等于分折评估、独立Reviewer或资格。已采纳的预测终态及失败编译可自动形成
一次原Thread反馈；先提交独立Turn预约，下次消费恢复同一Thread。预测反馈包含
原产物ID、origin、观察计数和最多首尾各16条观察，明确是否抽样，不冒充正式指标。
失败反馈只陈述公开原因，当前没有详细编译器诊断；修复应保留原实验父血缘。
重复消息不重复回送结果，未结算用量不继续调用模型；正式科学结论仍待接通。

032迁移为每个已成功结算的原生Turn保留唯一公开回答报告（qz.mission_summary）。
Worker读取锁定App Server原生summary视图，只有公开agentMessage、原Turn/item和
实际phase；phase缺失保留null。它不读取完整items/rollout或推理，报告不当作科学
指标或资格。读取/发表失败保留已结算用量，重投恢复原Thread补同一报告，不新开
付费Turn。报告占原Mission输出额度；原文冲突或超额不静默覆盖/截断。
全部Turn有完整用量、成功回答齐全、提案及科学任务均处理且反馈已在原Thread
得到公开回答后，Worker复用原Run终态事务收束本次会话，随后归档PGMQ。终态
已提交但ACK失败时只重放归档；取消先提交则不会再报成功。未知用量/科学终态
仍保留消息。这个SUCCEEDED仅是会话执行结果，不改Cycle/实验裁决、不创建资格；
原Attempt结果引用指向已有公开摘要，不是OCI任务的qz.job_result。

原生Turn失败或中断时，即使先前已显示部分token用量，也不能据此确认整轮用量；工具续轮的后续请求可能已经发出却没有用量回执。驱动保留真实失败/中断终态和未结算预约，不补零、不按早先部分数字退款或自动重发。原生COMPLETED且同轮用量完整可见时才进入当前驱动的自动结算路径。

锁定Codex的Turn列表可能把断流失败重建为Completed，不能据此认定成功。QZ只以真实终态通知或已经保存的同一通知确认结果；丢失通知且没有记录时保留UNKNOWN/预约，列表“已完成”不触发自动结算、退款或重发。

研究Mission首轮请求使用冻结Brief与同Cycle剩余token额度，不自动选择另一Profile或扩大预算。完整原生用量结算后可使用剩余额度；结果未知时仍占用原预约，重试不会换请求或再插一轮。设置了费用上限但原生计费不可用时停止首轮准备，不假造价格。常驻Worker可显式启用Mission消费（完整参数见CLI）；它以独立容量领取、续约、准备首轮并驱动原生账本。仅首轮准备/模型回复不等于完整研究完成；全部执行与反馈对账后才按上述条件收束会话。正式评估及Cycle科学结论仍在开发。

运行中的Turn用量达到本轮预约后，Worker先记录`mission.token_limit`和取消意图，再请求原生中断；事件里的用量只是首次达到阈值的观察，不是最终账单。缺最终回执时保留预约并阻止同Cycle的新模型支出。用量通知及中断是异步的，仍可能超额，不能视作逐token硬限额或严格美元限额。Codex的实验rollout budget跟踪/提醒不替代这条停止路径。

Mission使用不同于科学任务的队列选择，但共用现有PGMQ、Run/Attempt和租约。原生Thread回执一旦绑定不能替换，原生创建应答未知时不盲目新建；首轮预约不等于模型已经RUNNING。首轮和上述科学反馈已接常驻Worker，但完整评估及研究阶段收束仍须完成后才能作为整条产品流程使用，不应手工改库补成功状态。

Mission总Turn和修复Turn计数沿用原生App Server Turn，不是Provider HTTP请求计数。同一Turn的内部工具续请求仍占原预约、累计全部已观察token；QZ回送科学结果或要求修复的新Turn才单独预约。该计数不承诺限制内部HTTP请求数量，未知用量和超额仍按上述规则保留和停止。

Universe的 `registration_state` 必须同时展示：`NATIVE_METADATA` 表示存在正式登记证据，`LEGACY_UNVERIFIED` 表示历史记录尚未核验。该标记不证明真实市场来源、PIT或科学有效性，不能把历史行静默显示成原生登记。原生登记同身份重放比较收到的JSON内容，合法时间字符串原样保存；源origin等身份内容变化返回409，而非生成新版本绕过历史。

### Mission 原生资源前置条件

可信Mission启动器需要Linux cgroup v2、`/usr/bin/systemd-run`、`/usr/bin/prlimit`和
`/usr/bin/systemctl`，以及
当前服务用户的systemd manager；服务环境须提供该用户真实的`XDG_RUNTIME_DIR`。
建议按同用户systemd服务运行，缺失时明确不可用，不能退回无配额进程。
每Run的原生scope限制整个进程树的CPU速率、内存、进程数和剩余墙钟；新连接不重置
Run期限，已有scope未退出时不能创建第二份。prlimit的原生单文件上限是64MiB，
包括Codex内部SQLite/WAL/rollout；达到上限保留文件并报告不可用，不删除原生历史。
正式研究输出总字节仍严格使用冻结预算，不能拿原生文件上限代替或扩大研究预算，
两者不等于工作区总磁盘配额。
停止后的原scope由systemd异步回收；重开同一Mission前最多等3秒实际回收状态，
仍活跃就拒绝，不靠新scope名并行启动另一份，也不按名字杀掉旧owner。
这些也是`server worker`自动Mission消费的前置条件；缺失不能绕过资源限制启动。

## 不可变研究准备与数据撤销

研究准备入口为 `/api/v2/input-sets` 和 `/api/v2/evaluation-policies`，详情和
权限见 CLI 与 native-generated OpenAPI。InputSet 头、全部成员、冻结时间及
幂等回执一次提交；policy 与精确 experiment_family 同事务登记。登记不是
原生算法能力检查或研究任务执行，也不把 FIXTURE/PIT_UNVERIFIED 改成合格数据。
同项目已冻结对象可读取历史元数据；新登记会重新检查当前可用性，不使用历史
读权限代替新任务准入。

新输入和政策按稳定顺序锁定项目、数据源、运行端、数据使用许可；许可撤销
插入也取得同一 grant 行锁。停用/撤销先胜出时，等待后的创建请求会拒绝；
创建先胜出时，完成的冻结事实保留，之后的任务仍须重新核验许可。查询只返回
元数据，Sealed 原始字节依然不属于研究身份权限。禁止把许可撤销记录删除，
或重新登记同一原生对象为另一个分区以获得新资格。

增量迁移 `202609060012_research_contracts.sql` 保留旧迁移字节，增加研究准备
命令的封闭授权、撤销串行化和查询索引；同时修复原生版本触发器收到零个参数时
NULL TG_ARGV 导致 Runtime/Downstream 正常更新失败的问题。身份、已绑定来源、
created_at 仍不可变，revision 仍必须递增且不得溢出。升级使用前述完整 migrate
入口和停写/备份流程，不在业务请求中跑 DDL。

## 研究产物存储与018升级

`init-state` 创建私有 `artifacts` 子目录；`serve` 必须能够打开它，旧状态目录升级时只会
创建此前不存在的空目录。已有目录必须是非符号链接的私有目录；不会替操作者放宽或
修复权限。产物保存为原始字节，不属于 SecretVault 加密对象；宿主卷和备份必须限制
访问，不将此目录挂进研究 Agent 或任意 job。Secret/TOTP/model token仍不得作为研究
产物上传。状态卷的容量告警和空间预算不能省略。

本地对象在完整写入、只读同步和原子发布后，才在数据库提交元数据/原始命令回执。
文件成功但数据库提交未知时，保留文件并用相同 Idempotency-Key 与相同字节重试核对；
不能见到文件就手工补数据库，也不能因客户端断开就删除文件。未引用的私有 pending/
对象可能留作孤儿，回收必须在停止写入后核验原生目录与数据库引用；当前没有在线自动
删除产物的管理接口。数据库与产物卷应在停止写入的同一维护窗口备份/恢复，单独恢复
数据库不保证内容仍可读。下载缺失/损坏/权限不符时返回503，不返回另一个对象的内容。

新增迁移 `202609080018_artifact_submission.sql` 为机器凭据增加不可变的
issuer_attempt_id；新 Mission 签发由数据库锁住并绑定精确当前 Attempt。统一鉴权对
所有 Mission scope（包括读取和机器身份自省）要求该列非空且等于当前 Attempt。
旧凭据不猜测历史归属、原样保留 null 作为审计记录，但升级后不再拥有 Mission 权限；
由可信 Mission 服务重新签发当前 Attempt 凭据，普通 Operator/Agent接口不能伪造该列。
已绑定旧 Attempt 的凭据在接管为新 Attempt 后同样失效，不能直连 HTTP 恢复权限。
非 Mission 身份不因此失效。升级沿用停写、备份、正式 migrate 流程，不编辑旧迁移。

每进程最多四个上传/下载，上传文本最多2 MiB UTF-8、HTTP转义体最多12 MiB+16 KiB。
本地读取上限64 MiB；下载流在完成或断开前保留容量许可，慢客户端不会释放大缓冲的
占用后继续堆积请求。429遵守Retry-After。Mission全部历史上传按同一Run的冻结输出
额度累计；跨Attempt不能清零。产物接口只保存研究内容，不产生原生评估或交付资格。

## 数据和密钥

私有状态目录包含 master.key、session-key.ref、secrets。master key 为0600的32字节原生随机密钥；每个 secret 使用 RustCrypto XChaCha20Poly1305、独立随机 nonce，并绑定 UUID 和用途。加密对象先同步、只读发布，再写数据库引用。不要把 master key 放进普通数据库备份、源码、Agent workspace 或 job 容器。密钥丢失无法靠数据库恢复，需要独立安全备份。

机器凭据签发先持有数据库命令事务，确认不是重试后才生成 verifier 文件；并发同键请求不会重复生成。文件成功而数据库失败或提交结果未知时，清理重新取得相同数据库权限行锁，主库确认没有任何历史凭据引用才删除该 UUID 的原生认证 MACHINE_VERIFIER 对象。数据库不可判定时保留对象，不冒险删有效凭据。进程崩溃或取消后的孤儿可以通过本机维护命令回收：

```sh
cargo run --locked -p server -- prune-unpublished-verifiers --state-dir ./var
```

此命令仅删除可用当前密钥认证、用途精确为 MACHINE_VERIFIER 且没有历史凭据引用的对象；已撤销/到期凭据的 verifier、TOTP、Session key、其他用途、符号链接和损坏文件均保留。失败应先恢复主库/状态目录可用性后重试，不手工批量删除 secrets。输出只含回收数量，不含密钥或文件内容。

源码删除不授权删除运行中的旧库、用户 artifacts、备份或 Codex profile。不得将新 schema 直接应用到旧库；实际产品切换仍须完成只读导入、备份恢复和回滚演练。当前没有声称达到 RPO/RTO。

## 尚待完成的产品部署验收

研究/组合/交付 UI、Worker/MCP/Codex 真闭环、受信任 runtime 与 job 隔离、多 Alpha/共享资金、Paper/Live/Forward/Wake，以及完整恢复/迁移仍未完成。普通 PR CI 不携带生产秘密，真实受保护验收只运行经过审查的固定 Head。QZ 不持有 Broker 凭据或真实执行控制权。

### 完整迁移命令的提交边界

`cargo run --locked -p server -- migrate --application-role '<已创建的运行角色>'`
在一个专用连接/外层事务内运行完整领域与原生 session DDL、验证表合同并授予
运行角色 DML 权限，最后一次性提交。执行前停止应用写入并完成备份；这不是
零停机承诺。不再额外运行独立的 `PostgresStore::migrate()`。角色不存在、既有
session 表不兼容或任一授权失败时，不保留半次升级及 epoch 失效副作用。
已有数据/会话不会被删表“修复”。网络在 COMMIT 阶段断开时结果未知，应在
主库重连后通过原生迁移记录和权限复核，不直接宣称回滚或重复恢复备份。

原生服务 schema 也必须保持最低权限：`tower_sessions` 和 `pgmq` 的对象 owner、
CREATE、TRUNCATE、TRIGGER 与 `app` 一样禁止，角色继承或 SET ROLE 不能绕过。
缺少任何服务 schema 时先完成迁移，不用高权限运行账号让服务勉强启动。

迁移还会拒绝改变会话读写/持久性的已有表定义，包括 UNLOGGED、RLS/策略、
CHECK/额外唯一性、触发器、规则、继承及降低时间精度。错误为
`native_session_schema_incompatible`，原会话字节、epoch、旧迁移记录全部保留。
应先检查并由操作者明确处理结构冲突再重跑，不删除用户会话或绕过检查。
使用原生默认排序/opclass 的简单非唯一 B-tree（例如 expiry_date 查询索引）允许保留。

## Run admission / Attempt / SSE 运维边界

新增迁移 `202609060011_run_lifecycle.sql` 由既有原生 SQLx 部署事务执行：添加
不可变 admission/terminal receipt 及约束，不重写已应用迁移。运行身份仍只有现有
最小 DML；不能依赖管理员身份规避保护。升级新增的两张表由同一既有授权阶段处理。

队列重复出现不等于重新执行许可。先看当前 Run/Attempt/owner_epoch、域租约和
发送意图；SENT_UNKNOWN 或 ACKNOWLEDGED 应查询稳定外部 ID，不盲目重新提交。
接管保留既有 Attempt 和冻结的 runtime 配置。未知或未确认的取消保留待对账状态；
不要删除 Run/回执、手工降低计数、清空 PGMQ 或将未知结果改成成功。

正式结果回执完成后才允许 archive；archive 响应丢失可按同一原生 message_id 重读
归档结果。实验次数转入已使用，失败/取消同样保留，CPU 预约为累计承诺不重复退款。
模型 token/费用仍由原有逐 Turn 账本独立结算，不把计算取消当成模型费用退款。

SSE 为每批最多16条的持久查询，不要求内存消息通知和 sticky session；每批重新核验
权限。反向代理不得缓存事件或业务 API，应允许 text/event-stream 与至少10秒心跳。
60秒连接期限与每进程32连接上限只控制浏览器读资源，不取消计算或下游交付。
运行角色凭据和 runtime_snapshot 中的 credential_ref 不向事件流或公开 Run DTO 输出。

这些是受信任 Store 与 HTTP 的已实现入口，不是远端 Job 网关/隔离容器、完整研究
调度、科学 PASS 或交付资格的验收。当前不提供任意任务 JSON、任意 URL 或任意
命令的公开 enqueue/terminal 入口；尚未接通的研究服务必须使用同一准入事务。

## 增量升级：转移历史与请求超时

部署迁移使用专用连接，statement_timeout=0、lock_timeout=5s。长审计/回填不再
被普通请求的15秒 statement_timeout中断；锁冲突仍失败并整批回滚。请求池的
超时不因此放宽，迁移专用连接始终关闭。执行前完成备份、停止应用写入并排空旧事务。

017保存一次性 Handoff 转移记录；新领取与原生记录同事务提交。旧 CLAIMED/
ACKNOWLEDGED 被明确标为 LEGACY_CLAIMED_STATE，不冒充历史事件。旧 REJECTED
若带领取字段却无独立历史证明，或 Forward 消息早于领取、报告归属/角色不符，
升级失败并保留全部旧行和迁移历史。此时先保留库和原生下游记录供显式核对；禁止
删除反馈、回填猜测时间、将 FIXTURE 重标 REAL 或编辑已应用迁移来让检查变绿。

领域关系样例不是真实报告；正式 Forward 接入仍须验证报告字节、签发者和许可。
新消息仅接受已记录 CLAIMED/ACKNOWLEDGED 的精确项目 REAL/EVALUATOR_ONLY
qz.forward_report v1。先接收合法反馈后下游再拒绝时，保留转移事实和既有反馈，
停止该 Handoff 的新反馈接纳。

## Runtime 准入、凭据轮换与恢复边界

Runtime 的 `enabled` 和配置能力列表不代表可用。Cycle / standalone Run 准入和首次派发都要求当前配置 revision 的成功原生探测仍在有效期内，并核对实际 job kind、内存、墙钟时限与输出上限；拒绝不得扣减预算、入队或生成成功回执。探测有效期最多60秒，应通过正式探测入口刷新，不得手工修改观测表。CPU秒是累计预算，不是CPU核数。

Cycle 启动须明确提供 `researcher_profile` 和 `reviewer_profile`，各包含 Codex Profile 的 `profile_id` 与当前 `expected_revision`。两个选择随本周期冻结，不属于可重复使用的 Brief；可以明确选择同一 Profile，但研究和独立审阅使用不同 Thread。缺失、过期版本或正在登录/注销的配置不能启动。随后修改 Profile 不会修改旧周期或旧回执，也不能让旧周期自动采用新模型/账号配置；应以新选择启动新周期。历史没有选择的记录只保留原事实，不补造账号。

浏览器在研究项目的 Brief 行选择“冻结执行上下文”，分别选本项目 DISCOVERY、VALIDATION、SEALED 输入和已有 Runtime。草稿/暂停项目可以冻结；冻结会更新当前 Brief，但不会自动启用项目。随后使用“修改项目状态”明确启用，再从冻结版本选择“启动新 Cycle”，分别选择研究者与独立 Reviewer 配置并确认。超出 JavaScript 安全整数的修订号始终按原始字符串提交。断线或未知回执保留原请求内容和幂等键，可“重试同一请求”；关闭不代表撤销，重开前先核对 Brief 和“研究周期”记录。研究周期展示服务器状态、预算预约/使用及准备 Run，不将排队或准备成功显示为科学研究完成。

Runtime bearer 必须为32–8192字节、无空白的可打印ASCII；这只是最小线缆形状，不是随机性或熵保证。升级前登记的短Runtime凭据在原生传输构造时返回认证不可用，不会发送给远端。Operator应在真实Runtime端设置合适的新凭据，通过正式write-only Secret接口登记后更新Runtime引用并重新探测；不得用补字符、截断、降低验证或直接改数据库方式绕过。Downstream / Custom Provider保留各自上游支持的1–8192字节边界；TLS CA仍须通过原生PEM解析。

`PINNED_CA`新建必须提供非空CA引用且`development_http=false`；更新可省略或使用null保留已存CA。切换`SYSTEM_CA`时请求必须省略/null CA，由Store清除绑定，不能携带未使用的CA。不存在静默明文回退。

新探测失败或过期会拦截新任务，但不能证明旧任务停止。已进入`SENT_UNKNOWN`的Attempt必须继续按原始远端身份查询、取消和对账；不能因当前readiness不足就创建新Attempt、重跑研究、提前释放预留或把404当取消确认。并发同键探测只允许命令回执所有者发布一份原生快照；提交结果未知时保留可能已被引用的对象，不得以猜测为依据删除。真实任务执行、T01–T42、部署、备份与恢复的验收仍独立成立，探测成功不能替代这些证据。

## Brief 草稿成员权限

部署迁移仅对 `app.brief_data_bindings` 追加 DELETE，以支持同事务替换DRAFT成员；其他app表仍无DELETE授权。原生触发器锁住父Brief并拒绝FROZEN成员增删改，禁止移除触发器或授予TRUNCATE/TRIGGER。已部署实例运行正式 `server migrate --application-role ...` 补齐原生DML授权，而不是以数据库owner运行API。保存草稿不会执行模型、冻结Brief或发布资格。
