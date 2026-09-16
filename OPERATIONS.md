# 运行与部署

“组合 / 组合配置”打开原版本后可“请求组合构建”：选择运行中 Cycle、具备
PORTFOLIO_BUILD 能力的 Runtime、冻结 Forward 输入、PAPER/LIVE，以及原下游
权重快照或历史目标假设。成员按原 Alpha → 版本 → 资格选择，至少两个不同 Alpha，
聚合权重用十进制字符串填写且合计 1。资格窗口开放不等于当前准入；服务器重验
政策、生命周期、许可和原证据。LAST_TARGET 不代表实际持仓。资源限额可修改，
提交冻结原 Runtime 修订；响应未知时保留原内容及幂等键重试。202 仅登记 Build Run，
可查看运行，不代表已产生 Candidate、科学 PASS、审批或交付。

人工拒绝和重新考虑已接通HTTP/CLI，决定追加保存且绑定Candidate、下游及环境。
重新考虑只撤去这条人工拒绝的阻断，不恢复旧审批；已领取任务没有撤单/平仓效果。
请先读取最新决定后提交，冲突时重读；旧请求未知时保持原内容与幂等键。
网页Release详情可打开“人工拒绝与重新考虑”，查看原候选跨Release历史、
选择下游及环境并填写原因。停用下游仍可管理历史；重新考虑只作用于该范围最新REJECT，
不恢复旧审批，也不自动发送Offer。

目标包正文、Release创建/读取HTTP和CLI已接通。创建要求原Candidate及独立
PORTFOLIO/PASS，返回201只冻结版本，不批准Paper/Live或下游交付；完整成功链路仍待验收。
响应未知时保留原请求和幂等键重试，读取不会延长目标有效期。
浏览器在“组合”选择原候选及独立PORTFOLIO/PASS评估，确认“冻结目标包”；
服务器仍须重验原资格与许可。“交付”可按项目分页查看原Release与详情，
详情可“下载原始目标包”：核对原项目、PACKAGE 元数据与字节数后下载原始附件，不重新生成包；离线或记录读取失败时不可下载。DEMO 仍不能审批或登记 Offer。
REAL表示原来源，不是Live审批。“交付记录”分页展示原Handoff，详情保留原审批、
前版、交付序号、Claim编号及ACK时间；未确认保持未确认，读取不会续期或产生交易。
“Forward 证据”按项目分页显示下游原消息、修订和更正引用；详情读取原 Handoff/流的当前连续窗口，展示缺口原因、完整观测数和采纳消息。窗口连续不代表健康、资格或 Live 晋级；查看不会提交观测、交付或唤醒。
Release详情的“原审批历史”可分页查看原授权、证据集合、决定序号与下游版本；
历史缺失字段保持“历史未记录”，不据此提供发送资格。Release详情可选择“审批此目标包”，指定下游、Paper/Live及本地截止时间；
表单读取原下游版本与全部决定分页，当前人工拒绝不能审批。未知结果保持原请求/键重试，
201只保存审批。原审批历史中可为人工审批“登记 Offer”，指定截止时间；表单从全部原交付分页按
精确序号选择同项目/Mandate/下游/环境前版。重复版本或已领取候选不能再发，
未知响应保持原审批、前版、期限及幂等键。成功仅登记Offer，实际Claim/ACK看交付记录。
原审批历史的“撤销审批”展示原撤销记录，可追加立即或预约撤销；绑定最新原记录，
预约不得推迟已有最早生效时间。未知响应同请求/键重试，成功不抹除Claim/ACK。
“交付 / 自动化政策”可分页读取原版本并冻结新授权：明确选择模式、组合配置、
下游、样本/时长/日额度、独立晋级与维持指标及截止时间。默认不启用新再平衡，
勾选后才在政策中启用；提交绑定原项目版本，未知结果同完整请求/键重试。
政策登记不等于Worker已运行或交付成功。Worker可按非MANUAL调仓日程，以当前Mandate最新原Release的固定成员、Runtime修订、
环境和限额登记新Build；要求原Cycle仍运行、冻结政策有效、同项目新Forward原生cutoff
确实前进，并有政策原下游在该时点可用的同环境权重快照。旧数据换InputSet编号不会
推进cutoff；缺少来源不会改用历史目标假设。相同实际cutoff只自动登记一次，有待处理
成功候选时不继续堆积Build，每日尝试和已有交付仍受政策额度限制。运行页可查看Run。
日历模式使用原Dataset登记的会话及偏移；原表缺失或名称、版本、内容变化会拒绝准入，不根据周历补出交易日。
成功候选会在原政策仍有效且未被替换时自动进入一次独立Study，沿用原Cycle、Runtime修订、限额和冻结研究计划。
正式PORTFOLIO/VALID/PASS发布后自动冻结一个新Release；失败或不确定研究保留原记录，不重复堆积。
每阶段重验原来源和有效期，新包不会改写旧包，也不代表审批或交付完成。原政策行可“撤销政策”：读取原撤销历史，绑定最新记录追加立即或预约撤销，
不能推迟既有最早生效时间。响应未知同请求/键重试，已领取事实保留。

当前单基础币种 Nautilus 0.63.0 配置中，CurrencyPair 必须明确选择 MARGIN；
Equity 支持 CASH 或 MARGIN。系统拒绝不支持的账户/资产类组合，不自动切换账户
模型或改写旧执行假设；这只是模拟配置，不涉及真实券商账户。

“组合”选择项目后，“评估政策”可分页查看原版本或填写新不可变政策。
Validation、Sealed、组合阈值分别填写；未定义组合要求时保持null。所有数值、
方法和来源由操作者明确指定，保存不启动研究、不审批交付。网络结果未知时
保留原表单重试；关闭不撤销已发送命令，旧政策没有原地编辑入口。

`client portfolio candidate list PROJECT_UUID` / `show CANDIDATE_UUID`查询已发布
Candidate原始头、成员和目标快照。需要Operator或精确项目RESEARCH_READ的CLI；
执行、求解和证据状态分别保留，历史VALID不表示当前资格或交付授权，空目标不补权重。
浏览器“组合”选择项目后打开“候选快照”，可分页、刷新和查看同一原始详情。
详情中的候选评估列表对应`client portfolio candidate evaluations CANDIDATE_UUID`；
评估详情和指标复用`client evidence show/metrics EVALUATION_UUID`。只展示原始关联的
已发表保持研究证据，不读取报告字节；缺值原因、方法、单位、年化因子与原有效期保留，
缺值不是0，历史科学决策不是资格或交付批准，也不会自动启动模拟。
构建请求的current_weights_source可明确选择FORWARD_SNAPSHOT或LAST_TARGET。
后者使用同项目原Candidate的目标文件与子项，不延长期限、不冒称账户仓位。

本分支已实现 Rust 原生组件、逐轮 PostgreSQL Store 和可运行的浏览器认证 API，**尚非完整研究与交付产品**。旧实现已删除，无兼容服务；完整目标和完成条件在 DESIGN。

## 原生计算 Runtime 的独立运行边界

bar-notional/1镜像在非Sealed DATA_VALIDATE中记录最后一根已知BAR的原价格、
成交量和Nautilus原生名义金额；未测量为null，零成交量不补常数。Sealed不输出
这些明细。历史收盘估值不等于未来盘口、完整成本或DATA_BACKED资格；当前参与率
入口仍须完成原生来源消费链，不因质量任务成功自动放开。

受管组合镜像新增portfolio-weights/1：当前权重必须以独立REPORT产物与冻结副本
一起提供，原生任务核对内容、币种、资产顺序、总和和有效时间。LAST_TARGET不能
冒称下游快照；缺来源不补全现金。当前这条原生检查不替代Store正式资格与下游来源准入。

原生模拟镜像新增simulation-models/1：模拟请求必须冻结实际费用、填充/滑点、
延迟模型及种子，不能只给顶层插入延迟或依赖默认填充。具体字段见CLI与DESIGN A2。
模型选择使用已锁定Nautilus组件，不是费用来源或正式执行假设已经获准的证明。

Operator可通过`client portfolio assumptions create/list/show`管理新的不可变执行假设
（请求与HTTP路径见CLI）。先登记原生目录并冻结输入，再选择已探测Runtime；费用
必须与原资产定义一致。当前保存为保守BAR假设，不等同数据支持成本或可交付组合。
可选绑定原生DATA_VALIDATE历史单BAR报告，明确填写最长年龄与参与率；来源必须
匹配同一冻结输入、数据版本和Runtime，且未过期。详情返回原失效时刻，到期不能
自动续期或改写旧政策；历史成交量估值不是未来可成交保证。Store在Build准入和
Candidate发布时重读原来源并核对原期限，job核对原质量报告与逐资产量；需要
portfolio-liquidity/1镜像。来源损坏保留重试，到期不授新目标。
当前不支持将这些保守 BAR 假设标成 DATA_BACKED，不能手填绑定冒充可交付证据。独立组合 Study、评估发布和 Release 已有原生入口，使用方式见下文；真实数据全链验收仍未完成。
Build还要求portfolio-cost-source/1镜像：原费用文档随任务挂载，与冻结执行设置
完整匹配，发布再次核对原保存配置；修改副本不能绕过原费用来源。
本次Forward目录的原资产费率也必须匹配；费率变更须新建执行假设，不沿用旧费用求解。
原资产定义使用Rust InstrumentAny的外部标签Serde结构，不是Python式顶层type。
组约束从原Forward Universe成员记录的groups读取，按原决策时点核对生效与可用
时间，发布Candidate前再次读取原证据；不从资产名称猜分类。未提供/null是未知，
[]才是明确无组。有组约束时，每项资产必须有唯一且分类已知的成员记录，每个约束
组必须有参与资产；无组约束不强求分类。这不提升原数据或费用的资格。

下游可用自己的DOWNSTREAM/FORWARD_SUBMIT凭据通过`client forward-weights`
登记当前权重（字段和参数见CLI），项目/下游/环境由服务端核对；不能使用Operator
身份代报。保留external_message_id重试，改内容会冲突而非覆盖原报告。此入口只登记
target-only权重，不收账户或NAV；PAPER为SYNTHETIC，LIVE不自动获得研究资格。

`client portfolio build`使用原Mandate、运行中Cycle、原资格和下游快照引用，需
精确人工授权，参数见CLI。202仅表示Run入队；Worker采纳原生结果并完成发布核对后才可读取Candidate。已有原生事务与受控科学结果测试，真实授权市场数据的完整验收仍未完成。保守BAR非零滑点需portfolio-slippage/1，按原概率与最后BAR/tick换算
规划期望成本（公式及向上舍入见DESIGN A5.2）；不是未来成本上界或DATA_BACKED。
实际模拟继续使用原模型和原费率，不二次扣规划成本，不放宽其他来源/流动性约束。
原生candidate-simulation/2可在原目标有效区间内保持该目标模拟，读取原目标与费用
文件，不将最终权重放回产生之前。portfolio-sequence/1另支持原目标文件序列，
按每项原可用时间进入同一原生账户；不代表独立PORTFOLIO评估或Release已通过。
portfolio-study/6另提供原模型驱动的离线滚动原生计算，不依赖历史Candidate：
各截止使用历史前缀、实际模拟权益和权重，在同一个账户求解与执行。
同任务产物包含原源质量、JSON研究报告和Arrow历史目标文件；纳秒及精确小数
不经过浮点转换，失败帧保持null，三个产物必须一致。
可选滚动BAR政策逐截止测量原生历史量，并按专属年龄和原参与率规划；不能复用
过期快照、手填成交额或声称未来深度。零量可能使原问题不可行，不放宽约束。
支持固定间隔或原参数中冻结的手动截止；不排序、补点或挑选完成后的有利片段。
日历模式另需原完整会话文件，匹配目录日历名称/版本，截止取原收盘时间加显式偏移；
不会自动推断交易所休市或下载日历。原 Runtime 元数据可随 Dataset 登记完整会话，
Universe 返回 calendar_artifact_id；未登记时不补默认日历。Study 必须逐值匹配该原表，
登记不证明交易所准确性或 REAL/PIT 资格。正式组合评估通过下文的原计划 Study 入口准入与发布，不能直接把本机计算产物登记成正式评估。
该本机计算入口不可当作Operator操作或生产资格。
它不是恢复真实持仓，也不是已交付的Evaluation
或Release入口；可信Worker的保持研究评估与正式策略滚动评估/交付分开，后者仍待验收。
`client portfolio simulate`通过原来源准入申请保持模拟Run（参数见CLI），需
精确Candidate的人工授权。不要手工提交原生任务、回填可用时间或把202当作评估；
完整科学/评估发布及交付链仍待验收。
组合评估条件需在新政策的portfolio_metric_requirements单独冻结；null不含组合
PASS条件，不能借用Alpha/Sealed阈值。保存条件本身不是组合评估通过。
原生指标适配保留日均收益、252日年化波动率和Sharpe原值及来源；全现金的真实
零收益与日内不足样本分别处理。方法口径见CLI/DESIGN，尚非自动评估发布或交付。
模拟请求与原Candidate/政策/数据版本随Run不可变保存；重试使用原请求，不会
自动换成新政策或新目标。这是恢复关联，不表示评估已经通过。
candidate-simulation/2分别输出完整原登记窗口的数据质量报告和实际保持窗口的
模拟报告；前者核对载入覆盖，后者提供日收益，不混用两种计数。
Worker现从原模拟任务发表不可变FORWARD保持研究评估，再确认原队列消息；
发表或来源复核失败会重试。该评估不是Release监控窗口，不自动审批或交付。
成功进程仍可能因缺条件或日样本不足得到INCONCLUSIVE。

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

受保护的原生账号验收使用仓库内的 `protected_codex_login` 示例。先按 README 安装锁定的
Codex0.144.4；在仓库根目录执行以下命令，可验证真实设备登录启动、取消、重启空账号状态及空账号退出的幂等行为：

```sh
cargo run --locked -p server --example protected_codex_login -- \
  --codex-binary "$PWD/runtimes/codex/node_modules/@openai/codex-linux-x64/vendor/x86_64-unknown-linux-musl/bin/codex" \
  --cancel-only
```

省略 `--cancel-only` 可运行完整原生登录检查；必须由操作人在未录制的私人交互终端执行。
设备码和官方验证地址只显示在该终端，五分钟内完成授权。输入或输出被重定向时会在启动
前拒绝完整检查。不要将设备码、终端录屏或原生认证文件发送到聊天、CI、Issue或PR。
命令使用新建临时profile及原生file凭据存储，保留原生OAuth所有权；成功登录后重启
App Server核对持久状态，再退出并重启确认注销，正常结束删除该临时profile。
不读取既有profile、auth.json或账户令牌，也不执行推理。Ctrl+C或超时会按原登录ID请求取消并核对原生完成通知，已观察到的成功优先。
随后先关闭原登录进程，再用独立的新进程清理临时账号并确认退出状态；系统强制终止不保证临时目录清理。

`--cancel-only` 的结果必须保留 `logout_restart=not_run full_login=not_run`；
`empty_account_logout=passed` 只证明空账号退出的幂等路径，不证明已登录凭据被移除，不能记为完整T07通过。
此示例补充[官方App Server账号协议](https://learn.chatgpt.com/docs/app-server)的原生运行证据；
QZ网页账号操作、真实模型科学任务、同Thread消费结果和T42仍须分别验收。


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
新增Agent任意执行命令。编译是原试验首阶段的DATA_VALIDATE，不读取市场分区，
预约一次试验且失败/取消仍留账；MODEL只可来自原生生产者，不能把编译成功当成Alpha资格。可信Store已
编写原子准备入口，不能手工补库宣称任务完成。

031迁移增加提案到Discovery预测Run的不可变关联。可信准备入口只使用原提案编译
成功后采纳的MODEL和参数明确选择的冻结Discovery版本，沿用原编译的一次试验；重放
保留同一Run，不再扣额度。PARAMETERS格式见CLI，当前只支持Brief的FIXED_BARS
horizon。启用Mission的Worker在最新Turn结算后，每次消费按ordinal准备一个就绪的
编译/预测/正式验证步骤，复用原Mission JobLimits及当前累计预算；科学Worker独立执行。
每步前以当前Mission租约刷新已过期/临近过期的原Runtime能力探测，复用已登记
目标与Vault引用；原生HTTP在数据库事务外执行，60秒有效期不因模型耗时而延长。
原生调用未知或仅有部分usage时不开始这些步骤；已排队的任务不重建。预测成功
仍不等于分折评估、独立Reviewer或资格。编译或Discovery失败可自动形成一次原Thread
反馈；预测成功则继续正式Validation，待评估发表后才准备一次结果Turn。先提交
原预算预约，下次消费恢复同一Thread；不为中间观察抽样新增模型请求。
失败反馈只陈述公开原因，当前没有详细编译器诊断；修复应保留原实验父血缘。
重复消息不重复回送结果，未结算用量不继续调用模型；独立Reviewer及封存资格裁决见下文。原生队列测试不替代真实账号模型与科学任务的完整验收。

034迁移统一首阶段记账：新编译必须为1、新预测为0，且后者须引用原已计数编译。
各阶段仍累计CPU、输出、墙钟预算。已有账目不修改或退款；没有原始首阶段账目的
历史编译不能自动新增免费预测，须保留历史并明确报告该限制。

本地原生`job validate-alpha`执行已授权目录/模型的独立分折计算，命令与严格请求见CLI。
它重用原生切分/OLS/IC/RMSE，不携带Discovery或训练模型状态进入测试；保留所有折和
缺失指标，不产生数据库Evaluation/Qualification。其标签和训练索引仍属受限数值
证据；不能将stdout直接发给研究Agent或把手工运行当作完整自动链路。

可信本机`job evaluate-sealed-alpha`应用已冻结校准，不在封存数据上训练；保留
原始分数和校准收益。它只是受限数值入口，尚不代表已接通Runtime受管Sealed、
暴露预约或资格；不能将原始报告交给研究Agent。请求和限额见CLI。

Runtime受管`VALIDATE_ALPHA`沿用同一计算入口，仅消费登记的VALIDATION目录和
原MODEL/PARAMETERS，成功封口为`qz.alpha_validation.v1`。采纳会按原冻结请求
重建并逐一比较所有原生分折；少折、改索引或不一致的重复标签均拒绝，不产生资格。
原生Job镜像须按当前Dockerfile重建并登记其新OCI原生ID；旧native-stack标签缺少
`alpha-validation/1`，Runtime会报告镜像合同不匹配，不虚报支持新操作。
可信指标转换保留每个资产/折，并给出原生方法、单位、bar规格/horizon和真实配对
样本数供冻结阈值检查；没有全局平均或隐式年化，也不等于数据库评估已发表。

冻结Brief和启动Cycle会核对原生方法版本及Validation目录元数据：Selection必须
使用实际支持的PEARSON_IC或RETURN_RMSE、`asset:{a}/fold:{f}`及准确bar规格/horizon。
当前受管验证只接受一个已登记Validation版本（可多资产）、固定bars和WALK_FORWARD
选择类别；不能用total、未实现方法或多版本输入假装完成评估。旧政策不会被自动改写。

可信Store的正式验证准入复用原MODEL与原试验，只改用冻结Validation输入和政策，
不再计第二次试验；参数和原始报告保持EVALUATOR_ONLY。镜像漂移会拦截新的预测/
验证任务；既有Run重放保持原身份。原生Worker在正式Validation终态采纳后、ACK前
原子发表Evaluation、全部逐折指标和试验首次结论。文件/事务失败保持队列可重试，
不重跑试验；失败/取消亦为INCONCLUSIVE。原报告、实际方法和来源保留，指标缺失、
登记行缺失超限、样本不足或过期不能PASS。详细合同见DESIGN A4.8。Mission自动
发起此阶段，受控反馈只投影已封口评估和冻结选择指标的来源/口径/样本/有效期，
不读取EVALUATOR_ONLY报告内容。原Thread未回答、验证未终结或评估未发表时不能
收束Mission；后续独立Reviewer与封存资格裁决按下文继续。排队、单个Mission成功和完整研究链验收是不同事实，不能互相替代。

正式Validation发布还冻结每资产最后原生折的可用SCORE校准（DESIGN A4.4）。
校准MODEL/记录与原Evaluation同事务提交，失败保留原队列；训练截止保留原始
纳秒，DB时间向上取整到微秒。不会重拟合测试或Sealed标签、挑较好折、回填旧
报告、改变原AlphaVersion或把原REJECT改成PASS。同事务为原Alpha创建附加校准的
下一不可变版本，仅在仍为RESEARCH且活动指针未被改动时推进指针。原信号仍为
SCORE，不能忽略校准直接当收益；新版本不继承源版本的评估或资格。版本详情可
只读查看校准元数据及源版本Validation，训练截止明确为向上取整微秒，不下载模型。
Sealed/资格仍须单独完成；模型存在本身不是研究完成。
版本详情的“查看原资格历史”按版本分页显示原授予与最早撤销，包含过期与未来撤销。
“观察时刻开放”仅表示服务端观察时刻在授予期限内且撤销未生效，不证明当前政策、
生命周期或许可证有效，不能用作组合准入或交付批准。刷新失败明确显示失败而非空成功。

032迁移为每个已成功结算的原生Turn保留唯一公开回答报告（qz.mission_summary）。
Worker读取锁定App Server原生summary视图，只有公开agentMessage、原Turn/item和
实际phase；phase缺失保留null。它不读取完整items/rollout或推理，报告不当作科学
指标或资格。读取/发表失败保留已结算用量，重投恢复原Thread补同一报告，不新开
付费Turn。报告占原Mission输出额度；原文冲突或超额不静默覆盖/截断。
043迁移及共享发表器按原Mission角色保护请求和总结：Reviewer材料为EVALUATOR_ONLY，
不出现在普通产物读取中。045迁移增加原目标/Turn/公开回答的不可变审阅关联。
Reviewer与Researcher共用Runtime能力刷新约束；冻结版本变化须拒绝，不静默使用新配置。
研究者成功且选择COMPLETE、有非空原审阅目标时，确认事务使用Cycle冻结Reviewer
配置创建独立Run；同账号允许复用配置，但Thread/工作区不同，预算继续累计。
可信Worker复制原代码、原参数和有界Validation上下文，每目标一条原生审阅Turn。
不复制研究聊天、账户材料、Sealed原始行或校准系数；Reviewer不能上传或提案。
审阅JSON必须匹配原目标，缺失/错误格式记INCONCLUSIVE，不自动付费修补回答。
全部审阅和用量对账后，可信Worker按原排名为PASS目标准入Sealed任务；每次消费
最多一个，复用人工入口的原模型/参数/数据准备器，但不使用Operator授权。
046迁移将原审阅与原Sealed Run不可变关联；参数、任务、预算和队列同事务提交。
确认前须有全部必要关联，失败回滚后重投不重开模型或重复收费原试验。当前能力
先由真实Runtime探测刷新；耗时准备失败后也重验租约，旧Worker不能改Cycle状态。
预算耗尽明确结束Cycle，过期源证据或需修订输入进入WAITING_INPUT；取消不补做。
Sealed已入队不表示执行成功；科学Worker仍须发表正式评估，PASS不是审批或资格。
原独立Reviewer关联的封存ACK已接入资格裁决：三组原数据必须均为REAL、PIT已验证
且AS_KNOWN_THEN，原科学评估、许可及绑定仍须有效。资格期限不超过原证据/许可，
重复ACK不续期或重授撤销资格，也不恢复暂停/退役Alpha。FIXTURE即使科学PASS仍
不获资格。真实数据正向授予、资格查询/披露及交付使用链尚未验收，不应手工改库补证。
全部Turn有完整用量、成功回答齐全、提案及科学任务均处理且反馈已在原Thread
得到公开回答后，Worker复用原Run终态事务收束本次会话，随后归档PGMQ。终态
已提交但ACK失败时只重放归档；取消先提交则不会再报成功。未知用量/科学终态
仍保留消息。这个SUCCEEDED仅是会话执行结果，不表示Cycle完成/实验裁决、不创建资格；
原Attempt结果引用指向已有公开摘要，不是OCI任务的qz.job_result。
取消后Worker重启可重连原已登记Thread，对账原已发送Turn；不签发新Mission凭据、
不启用MCP、不准备后续实验或审阅轮。原资源上限和至多110秒清理窗口仍有效；
清理不是研究续期。原生列表状态或丢失的用量不能冒充最终回执，仍保留预约/队列，
也不会仅因重连成功就把CANCEL_REQUESTED改成CANCELLED。
取消或期限到达后，不再等待尚未创建的科学阶段/模型反馈，也不补造公开回答。
无发送意图的原Turn通过现有账本记NOT_SENT；已发送且缺最终用量的仍保持未知和
预算占用。已经准入的科学Run要等原生真实终态，不自动取消其他Run或下游；原
Validation发布消息独立保留。零模型预约可直接确认取消，不为此启动新Thread。
这仅结束本Mission的授权工作，不表示删除原生历史、放宽预算或取消未知远端任务。
成功Discovery预测可自动登记原生产者绑定的RESEARCH Alpha首版本，复用现有
命令回执防重复；MODEL/CODE、镜像、血缘及signal单位来自原任务和冻结Brief。
没有原生校准就保留null，不授资格或变更实验PENDING。033迁移允许已引用的
PENDING实验在新评估及指标的同事务内发表一次裁决；输入仍冻结，旧评估不能
事后补裁决，已有裁决不能改写。资格由前述独立Reviewer关联的封存ACK裁决；浏览器“Alpha”可查看原版本、正式Validation与资格历史，并明确请求封存评估。读取历史不授予当前资格。

原生Turn失败或中断时，即使先前已显示部分token用量，也不能据此确认整轮用量；工具续轮的后续请求可能已经发出却没有用量回执。驱动保留真实失败/中断终态和未结算预约，不补零、不按早先部分数字退款或自动重发。原生COMPLETED且同轮用量完整可见时才进入当前驱动的自动结算路径。

锁定Codex的Turn列表可能把断流失败重建为Completed，不能据此认定成功。QZ只以真实终态通知或已经保存的同一通知确认结果；丢失通知且没有记录时保留UNKNOWN/预约，列表“已完成”不触发自动结算、退款或重发。

研究Mission首轮请求使用冻结Brief与同Cycle剩余token额度，不自动选择另一Profile或扩大预算。完整原生用量结算后可使用剩余额度；结果未知时仍占用原预约，重试不会换请求或再插一轮。设置了费用上限但原生计费不可用时停止首轮准备，不假造价格。常驻Worker可显式启用Mission消费（完整参数见CLI）；它以独立容量领取、续约、准备首轮并驱动原生账本。仅首轮准备/模型回复不等于完整研究完成；全部执行与反馈对账后才按上述条件收束会话。后续Reviewer、封存评估与Cycle收束分别核对原关联和原科学结果，不能从研究Mission成功推断可交付；真实账号完整研究仍待验收。

运行中的Turn用量达到本轮预约后，Worker先记录`mission.token_limit`和取消意图，再请求原生中断；事件里的用量只是首次达到阈值的观察，不是最终账单。缺最终回执时保留预约并阻止同Cycle的新模型支出。用量通知及中断是异步的，仍可能超额，不能视作逐token硬限额或严格美元限额。Codex的实验rollout budget跟踪/提醒不替代这条停止路径。

Mission使用不同于科学任务的队列选择，但共用现有PGMQ、Run/Attempt和租约。原生Thread回执一旦绑定不能替换，原生创建应答未知时不盲目新建；首轮预约不等于模型已经RUNNING。首轮、上述科学反馈、独立Reviewer、封存评估和会话收束已接常驻Worker。代码接通及受控测试不等于真实账号、市场数据和交付全链已验收；不应手工改库补成功状态。

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

新评估政策必须分别声明Validation与Sealed指标要求；封存评估不借用分折阈值。
历史政策未定义Sealed要求时显示null，不能直接用于Sealed，也不会自动改写旧政策。
Brief冻结和Cycle启动要求Sealed使用真实资产的`asset:N`，不接受Validation折scope；
同时核对登记的Sealed与Validation资产/bar顺序，不读取封存市场行或启动任务。
内部受管Sealed操作仅运行原目录/Wasm/冻结校准；镜像需含alpha-sealed/1能力。
它不提供研究者自助准入，也不代表暴露预约、正式评估和资格完整链路已交付。
原生能力发放前按根血缘预约Sealed读取机会；失败/取消保留机会记录，
不能换项目、政策或Dataset UUID重置已使用的额度或既有暴露。
封存Run的原结果及全部指标正式入账后才能ACK；失败/取消也保留INCONCLUSIVE。
人工可用`alpha evaluate <版本ID>`在明确的运行中Cycle预算内请求封存评估；
须指定该Cycle冻结政策、Sealed输入、Runtime版本和资源限额，不重新收费原编译试验。
模型/校准由原验证派生，202只表示接受原Run，不是资格。完整请求与CLI授权见CLI。
封存机会耗尽或已有不相容披露时，尚未授能力且未发送的封存Run记录
SEALED_OPPORTUNITY_UNAVAILABLE，不等待墙钟耗尽；已有机会不退款，已发送任务仍需对账。
研究准备入口为 `/api/v2/input-sets` 和 `/api/v2/evaluation-policies`，详情和
权限见 CLI 与 native-generated OpenAPI。InputSet 头、全部成员、冻结时间及
幂等回执一次提交；policy 与精确 experiment_family 同事务登记。登记不是
原生算法能力检查或研究任务执行，也不把 FIXTURE/PIT_UNVERIFIED 改成合格数据。
同项目已冻结对象可读取历史元数据；新登记会重新检查当前可用性，不使用历史
读权限代替新任务准入。
PORTFOLIO输入保留原DISCOVERY/VALIDATION分区，至少需要研究加纸面用途许可。
它不是DISCOVERY输入的别名；读取器支持其原来源不表示正式组合研究已经准入。
评估政策表单可在独立组合阈值下冻结Study计划：原PORTFOLIO输入、研究起点，
以及手动模式的完整时点。结束取原数据结束，不填写任意结束或运行后挑选时段。
使用带时区且最多六位小数的时间文本；关闭手动模式/计划提交null，不带回旧列表。
计划不可修改，保存仍不启动研究；原调仓模式、TTL、模型和许可在正式准入再核对。

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

受管PORTFOLIO_BUILD已改为原FORWARD目录和MODEL产物生成预测/收益，再聚合及
Clarabel求解，不接受手填预测或历史收益。更新必须重建并验证登记的新镜像；
合成目录回归不代表Alpha拥有当前REAL资格、许可或可发布Candidate。
当前输入还须绑定原optimizer/alpha_ensemble的严格类名、版本和参数，替代顶层
settings；风险厌恶系数也冻结在optimizer.parameters，不再接收顶层risk_aversion。
对应镜像为portfolio-models/4，具体格式见CLI原生科学任务入口。独立allocate
仍可接受合成return_history作数值检查，但这不是受管任务或资格准入入口。
Mandate的真实API/CLI已支持新建不可变版本和读取，创建前须有当前有效Runtime探测
及一致的执行/政策引用。失败不落半条配置。在“组合”选择已有项目后，可新建配置
并查看服务器保存的不可变版本；当前须填写已有 Runtime、投资域、政策、执行假设
与费用产物的准确编号。金额和版本按字符串原样保存，不从真实账户读取。
同一项目的“执行假设”标签提供创建、分页与原版本详情。填写冻结输入、数据版本、
当前Runtime及明确的模型参数、逐资产费率；服务端核对原资产定义。随机种子与延迟
使用整数字符串，金额/费率使用十进制字符串；不会填入凭据、账户资产或默认费用。
详情包含原配置产物、能力探测、镜像、日历和结算引用。当前仅保存保守BAR假设，
可勾选历史单BAR来源并查看原报告、参与率、最长年龄与失效时刻；取消勾选会清空
该配置，不在下次保存暗中携带。历史量消费不等于DATA_BACKED或完整组合交付。
同一表单可选择互斥的“登记滚动 BAR 流动性政策”，明确每步最大年龄与参与率。
它与费用配置一起不可变保存，详情显示原政策文件编号；不是延长旧快照或手填市场量。
登记不启动研究。原计划Study可经client portfolio study或POST /api/v2/portfolio-studies
提交六字段意图，需原Operator授权与幂等键。也可在“组合→候选快照→请求组合 Study”
选择运行中Cycle、Runtime和资源上限；原政策计划只读，没有计划不能提交。
未知结果重试保持原内容、Runtime修订和幂等键；202只表示登记，不是交付资格。
原任务三份报告含Arrow历史均通过核对后，才按原组合政策记录评估；取消/失败或
不可行只保留不通过证据。有效期由政策、资格及许可决定，不延长或改写原结果。
原生portfolio-build-rolling/1可重读原政策并测量本次选择的历史BAR，输出与求解
资产绑定的bar_notionals；Store准入冻结原政策，Candidate发布重读政策和原BAR年龄。
发布时已过期则保留求解结果、Candidate为INVALID且无可用目标；这不构成组合研究资格。
离线不可提交；响应丢失时保留原输入重试，使用原幂等回执，不能将关闭窗口当作撤销。
当前支持方差/CVaR下的最小风险、最大效用和风险预算；完整Candidate交付尚未验收。
选择“风险预算”后，明确填写风险资产总敞口，以及原资产标识、份额和LONG/SHORT
目标方向；份额合计1，不是资本权重。只做多不能分配正份额给SHORT。需要新镜像
方差portfolio-risk-budget/1与SECOND_ORDER_CONE能力；CVaR需portfolio-cvar-risk-budget/1
与POWER_CONE及原CVAR能力。全部既有约束仍有效，冲突时不生成近似替代目标。
CVaR预算只接受有限正总风险，发布复核原生场景对偶及真实贡献，无法形成正预算时
无目标；不将负风险裁剪为正。保存配置也不等于预算已可行或可交付。
可填写正的“每决策周期风险上限”，不用时留空；不是标准差或年化波动率。
选择CVaR时必须自行填写大于0、小于1的置信水平（例如0.95，不是95），上限
表示同周期预期损失收益率，使用完整原始场景及精确尾部质量。需要重建登记
portfolio-cvar/1 与 LINEAR_PROGRAM 镜像；不以方差或默认置信水平替代。
选择方差时上限表示同周期收益方差，
这要求重建并登记 portfolio-variance-bound/1、SECOND_ORDER_CONE 镜像能力，
不沿用不支持该约束的旧探测。发布复核误差最多为上限乘敞口容差。

研究、组合、交付及Forward历史已提供Web/CLI操作面；Worker研究续轮、独立Reviewer、资格、组合发布及自动Paper/Live/Wake已有原生实现和分项测试。完整无凭据Demo、真实账号模型闭环、真实授权数据的Web/CLI新实例链路、用户旧备份迁移及完整恢复演练仍待验收，具体缺项见README“尚未完成的交付验收”。普通 PR CI 不携带生产秘密，真实受保护验收只运行经过审查的固定 Head。QZ 不持有 Broker 凭据或真实执行控制权。

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
命令的公开 enqueue/terminal 入口；研究入口使用同一准入事务，不可绕过原授权直接入队。

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

下游内部原生能力读取固定为 `/downstream/v1/capabilities`，SYSTEM_CA TLS与部署限定
地址，禁止重定向/环境代理/自动重试；总请求10秒、响应64KiB。它只接受target-only
合同；`accepting_targets=false`原样保留，不伪装为可交付。使用 `client downstream probe <id>`
（精确DOWNSTREAM_PROBE人工grant）记录真实观察，`client downstream readiness <id>`只读。
需为serve和worker配置独立DOWNSTREAM_TARGETS（默认[]），不会继承Runtime允许列表。观察固定
在探测开始后60秒失效；更新配置后重新探测。原回执重放不刷新时间，较早探测的迟到
响应不能覆盖新探测失败。Worker会在原观察临近到期时，用每下游唯一短租约刷新未领取Offer或当前有效自动政策需要的观察；失败/崩溃后按原期限重试，不延长旧观察。人工审批与领取会重验该观察；完整交付链尚未验收，观察成功不是交付或真实交易验收。

人工审批使用 `client release approve RELEASE_UUID`，具体请求及单次人工grant见CLI。
服务端校验原Package与当前来源许可、期限、资格、下游版本/环境，并冻结原报告引用。
PAPER许可不能用于LIVE；审批期限不能超过原Release或当前来源期限。拒绝后的重新
考虑只允许申请新审批，不恢复旧审批。`client approval show APPROVAL_UUID`仅查看
原历史；配置版本改变后须重新审批。人工Offer使用 `client handoff offer`，绑定原审批并重验当前来源/下游；显式选择同项目/mandate/下游/环境的最新Offer作为前版，字段见CLI。
同一Release不能换键重复发送。新Offer只撤销尚未领取的前版，已领取事实不改写。
`client handoff show UUID`读取当前状态，原幂等回执不代替当前状态。下游使用 `client handoff claim UUID` 及其DOWNSTREAM_CLAIM凭据领取；external_claim_id必须与幂等键相同，200含原Package与转移。相同编号重试只取原回执，不能换编号再次领取。Worker补记尚未领取Offer的到期和审批撤销；已领取事实不改写。下游使用 `client handoff ack UUID` 与精确DOWNSTREAM_ACK凭据记录接受或拒绝，字段见CLI；原领取编号必须匹配，重复回执不产生新交付。Operator使用 `client approval revoke UUID` 与精确人工grant追加立即或未来撤销；最早生效日期不能被后续记录推迟，`client approval revocations UUID`查看原历史。自动 Paper/Live 原生消费已接通；浏览器提供政策、人工审批、Offer、撤销及交付历史。Claim/ACK仍由下游凭据通过原生HTTP/CLI执行，页面不能代领。

Cycle 启动须明确提供 `researcher_profile` 和 `reviewer_profile`，各包含 Codex Profile 的 `profile_id` 与当前 `expected_revision`。两个选择随本周期冻结，不属于可重复使用的 Brief；可以明确选择同一 Profile，但研究和独立审阅使用不同 Thread。缺失、过期版本或正在登录/注销的配置不能启动。随后修改 Profile 不会修改旧周期或旧回执，也不能让旧周期自动采用新模型/账号配置；应以新选择启动新周期。历史没有选择的记录只保留原事实，不补造账号。

浏览器在研究项目的 Brief 行选择“冻结执行上下文”，分别选本项目 DISCOVERY、VALIDATION、SEALED 输入和已有 Runtime。草稿/暂停项目可以冻结；冻结会更新当前 Brief，但不会自动启用项目。随后使用“修改项目状态”明确启用，再从冻结版本选择“启动新 Cycle”，分别选择研究者与独立 Reviewer 配置并确认。超出 JavaScript 安全整数的修订号始终按原始字符串提交。断线或未知回执保留原请求内容和幂等键，可“重试同一请求”；关闭不代表撤销，重开前先核对 Brief 和“研究周期”记录。研究周期展示服务器状态、预算预约/使用及准备 Run，不将排队或准备成功显示为科学研究完成。

Runtime bearer 必须为32–8192字节、无空白的可打印ASCII；这只是最小线缆形状，不是随机性或熵保证。升级前登记的短Runtime凭据在原生传输构造时返回认证不可用，不会发送给远端。Operator应在真实Runtime端设置合适的新凭据，通过正式write-only Secret接口登记后更新Runtime引用并重新探测；不得用补字符、截断、降低验证或直接改数据库方式绕过。Downstream / Custom Provider保留各自上游支持的1–8192字节边界；TLS CA仍须通过原生PEM解析。

`PINNED_CA`新建必须提供非空CA引用且`development_http=false`；更新可省略或使用null保留已存CA。切换`SYSTEM_CA`时请求必须省略/null CA，由Store清除绑定，不能携带未使用的CA。不存在静默明文回退。

新探测失败或过期会拦截新任务，但不能证明旧任务停止。已进入`SENT_UNKNOWN`的Attempt必须继续按原始远端身份查询、取消和对账；不能因当前readiness不足就创建新Attempt、重跑研究、提前释放预留或把404当取消确认。并发同键探测只允许命令回执所有者发布一份原生快照；提交结果未知时保留可能已被引用的对象，不得以猜测为依据删除。真实任务执行、T01–T42、部署、备份与恢复的验收仍独立成立，探测成功不能替代这些证据。

## Alpha 与正式 Validation 查看

原研究Mission收尾后，可从“研究周期”查看冻结试验选择；CLI为`cycle selection`
和`cycle trials`。快照保留同Family/根血缘已登记的完整试验历史、原执行状态、
排名和排除理由，不因失败、取消、未执行或落选删除试验。候选不足或存在尚未
完成的可比试验为INCONCLUSIVE；COMPLETE只表示冻结比较集合数量满足，不是
科学PASS、Sealed或可交付资格。未形成返回404；以后完成的历史试验只影响新的
Cycle快照，不回填旧排名。查看不重跑模型或改选赢家。
展开成员可分别查看原Validation版本和“冻结审阅版本（非资格）”。只有入选且原验证
PASS的有效版本可形成审阅目标；SCORE指向原验证附加校准的版本，不跟随active指针。
空目标不补造校准；旧快照不回填。非空也不代表Reviewer已运行或有Sealed/交付资格。

在“Alpha”选择项目，查看已登记Alpha、不可变版本、来源、单位、horizon、原实验和
校准引用；切换版本不修改活动版本。版本下只列出已正式发表、可披露的Validation。
评估分别显示执行状态、证据状态、科学决策及原有效期，指标可分页查看方法/单位/
频率/样本数和来源。缺值保留原因，不显示为0；无记录与请求失败分开呈现。
登记状态、科学PASS和未过期均不是当前可交付资格。页面不读取受限报告，也不
触发新的研究。CLI使用同名alpha/evidence只读命令和原生分页，须精确项目CLI授权；
Mission/Automation/Downstream不能借此读取额外证据。Sealed及独立Reviewer/资格
遵循各自合同，不能用本页的Validation结果替代。

## Brief 草稿成员权限

部署迁移仅对 `app.brief_data_bindings` 追加 DELETE，以支持同事务替换DRAFT成员；其他app表仍无DELETE授权。原生触发器锁住父Brief并拒绝FROZEN成员增删改，禁止移除触发器或授予TRUNCATE/TRIGGER。已部署实例运行正式 `server migrate --application-role ...` 补齐原生DML授权，而不是以数据库owner运行API。保存草稿不会执行模型、冻结Brief或发布资格。

冻结政策使用 `client automation authorize PROJECT_UUID` 及绑定完整请求的POLICY_AUTHORIZE人工授权，内容见CLI和DESIGN A7.0。保存会新建不可变版本并更新项目当前政策；`automation list/show`读取原历史。`automation revoke POLICY_UUID`通过POLICY_REVOKE授权追加立即或未来撤销，`automation revocations`查看记录。政策管理与原生自动Paper/Live消费已接通；登记成功不表示发生了审批或交付。

自动 Paper：ACTIVE 项目当前有效 AUTO_PAPER/AUTO_HANDOFF 政策由 Worker 轮询消费，原审批和 Offer 同事务产生。每日限额按数据库 UTC 日、原项目/下游及不同 Candidate 计数，包含人工记录；换政策版本不重置。政策替换、禁用或撤销阻止未领取记录继续领取，已领取事实不改写。`client handoff list PROJECT_UUID --limit 50`（可选 `--cursor UUID`）查询原绑定与当前状态；下游凭据仅见自己的记录。Live 自动晋级已接入同一 Worker，条件与证据边界见下段。

自动 Live：仅当前有效 AUTO_HANDOFF 政策可消费原 Candidate/下游的 Paper 观察。全部已报告原 Paper Handoff/stream 均须有当前原生 HEALTHY 观察，每个流分别满足样本数、完整窗口时长与两组指标；不合并样本或挑选有利流，超过255个流拒绝。审批与Offer同事务冻结完整排序的观察UUID集合；首次Claim重验同一集合、完整来源、Live数据用途、Release与下游readiness。新流、更正、撤权或过期会阻止旧证据继续授权；已有Claim重放保持原事实。同一Candidate当日Paper/Live合计占一次额度。人工Live审批行为不变；浏览器“交付”提供自动化政策、原审批/交付记录及Forward观察历史。完整市场与部署验收仍未完成。

Forward 报告：原 Handoff 领取后，精确项目/下游 FORWARD_SUBMIT 凭据使用 `client --idempotency-key MESSAGE_ID forward submit < forward-message.json` 提交 ForwardMessageSubmitV1（完整字段见 DESIGN A7.3）。external_message_id 必须与请求头/CLI的MESSAGE_ID一致，是原幂等编号，未知结果保持原报告重试；换编号重传相同逻辑消息也只返回原记录。纠正必须引用最新原消息、revision加1并保留窗口。三个时间使用UTC微秒精度；原始收益报告仅保存在EVALUATOR_ONLY Artifact，不能夹带账户/NAV/订单或执行权限字段。`client forward list PROJECT_UUID --limit 50 --cursor UUID`只读元数据；首次省略cursor，下游仅见自己的记录。收到报告不表示连续窗口、统计评估或Live晋级已通过；Worker分别执行原窗口评估和当前政策下的晋级检查，结果以原Evaluation、观察及交付记录为准。

`client forward window HANDOFF_UUID --stream STREAM`读取原始窗口来源投影：最新纠正版、连续性、合格样本数及缺序/partial/缺失/重叠原因。partial纠正不会回退旧complete版本，任一缺口或重叠将合格计数清零；原报告仍不公开。空stream返回NO_MESSAGES。此投影尚不是已封口的Forward统计评估，不授予自动Live或Wake资格；完整字段与原生来源/容量边界见DESIGN A7.4。

Forward收益频率：报告可声明 `returns_frequency=UTC_DAY`（完整UTC日，样本为右端午夜，complete不得缺日）或 `REPORTED_OBSERVATION`；缺失表示未知，不能推断日频/年化或独立样本。同stream含纠正历史的频率不一致时窗口返回 `FREQUENCY_MISMATCH`、合格计数为0。此来源检查尚不等于原生ForwardEvaluate完成。

原生 ForwardEvaluate 科学job已支持冻结原REPORT输入、原纠正链重验及nautilus-analysis 0.63.0的UTC日均值/波动率/Sharpe（365日年化）。Runtime仅在明确登记FORWARD_EVALUATE固定镜像后宣布该能力；不接受额外目录/模型。当前已具备受冻结政策约束的Store准入与原Run/PGMQ队列，已接原Worker终态Evaluation/window发布，自动调度已接通，原生观察、待处理Wake、受限Cycle消费及政策约束的Live晋级已接通，不把直接job结果当作Live/Wake资格；输入/结果/上限见DESIGN A7.5。

Forward可信准入仅供内部Worker调用：沿用原Candidate Runtime，完整原日频报告在项目锁内冻结为受限FORWARD输入，固定30CPU秒/60墙钟秒/512MiB/1MiB输出、无Cycle并发2，同来源重放不新增Run。纠正、撤销、替换或过期会阻止未发送任务；普通InputSet接口仍不能复制受限报告。原Worker会在终态采纳后、ACK前发表测量Evaluation与封口窗口，decision为INCONCLUSIVE；更正/撤权/过期或不完整统计不产生有效样本。Worker沿用五秒项目轮询，每轮至多预约一个原反馈流，失败三十秒后公平重试，新增/纠正可提前重试；相同来源不重复入队。Worker在ACK前按原政策追加原生观察：维持要求不通过为DEGRADED，维持通过但晋级要求不通过为WATCH，两组通过为HEALTHY，缺失/过期/无效为INSUFFICIENT_DATA。只有当前有效DEGRADED追加唯一待处理Wake；观察/Wake失败保留原消息重试。Wake消费复用原人工研究上下文及冷却/日额度；Live晋级消费完整原生HEALTHY观察集合，详见DESIGN A7.6–A7.11。


原生退化 Wake 由现有可信 Worker 自动化轮询消费，继承原 Candidate 研究 Cycle 的
人工 Brief/Profile 上下文并重验当前数据/运行时/政策。冷却与 UTC 日额度不足延后；
暂停保留待处理记录，更正/过期/撤权取消旧 Wake。与 DATA_VALIDATE/PGMQ/startup
同事务绑定唯一新 Cycle，不发放 Operator 身份或交付权限。无 Agent/HTTP 手动强制
消费入口；可用 `cargo test --locked -p store --test forward_evaluation --test cycles`
验证原生事务和受控协议，不能把该测试称为真实多日反馈/模型/OCI验收。

组合 Build/Study 的运行详情提供“自动再平衡来源”，可核对原政策、来源 Candidate、输入、Runtime 版本及后继 Study/Release。它展示历史事实，不代替当前政策或交付批准。CLI 使用 `server client run rebalance RUN_UUID` 查询相同记录。


旧库迁移准备先保留一致性备份，并在独立副本运行 `inspect-historical-source`（配置和输出字段见 CLI）。它以只读可重复读事务给出行数和原生外键孤立计数，不启动旧 Job、不修改原库；报告成功不等于导入完成。当前入口不复制数据/凭据，也不检查产物文件或授予旧策略资格。


旧产物复制使用 [CLI 的旧产物导出命令](CLI.md#旧产物的实际字节导出)。先保留原备份并审查私有选择清单，再导出到新目录；逐项核对报告，密封及待确认文件不读取。`COPIED` 证明实际字节复制和目标回读一致，不代表旧库完整迁移、旧策略资格或新审批权限。未支持/缺失文件留在原备份处理，不可通过删清单项目来宣称全量通过。


旧库非秘密行内容可通过 [原生 CSV 投影导出](CLI.md#旧库行数据的原生-csv-投影导出) 保留。它与产物复制共用稳定原安装身份，但二者报告分别核对，不由行数或复制成功推导完整迁移。先保留完整旧备份；逐列排除、未支持表、缺少表和实际外键问题必须处理后才能进入后续可信注册/导入验收。

内部历史投影持久化现已通过隔离 PostgreSQL 验证：报告、原身份关联及只读记录同事务提交，重复导入不创建新业务资格，冲突回滚。Linux 部署可信注册及 HTTP/CLI 入口见 [CLI](CLI.md#历史投影注册和导入)。操作前逐项核对报告，受控测试不证明用户旧备份、产物覆盖率或恢复验收已完成。


在网页“设置 → 历史迁移”使用部署者登记的导出编号，默认只试运行。请求结果未知时
保留原表单并重试同一请求；报告列表可查看原/投影行数、缺表、排除原因、未核验关系
和分页原身份映射。实际导入须明确取消“仅试运行”，仅保存只读历史，不赋予资格。
报告与映射可查询不代表真实旧备份、全部原字段、产物、密封沿袭或恢复验收已完成。

在历史报告的“原身份映射”展开一行，可查看已导入字段目录，再按字段逐段读取。
日期、数字和原状态保留为文本；NULL、空串明确区分。长字段用下一页继续，不执行
其中的脚本或旧任务。排除的字段仍保留原备份，不能从本入口读取；原产物及密封沿袭
验收仍须另外完成。


历史行导出的注册可同时指定同安装的原生产物导出目录（CLI 的 artifact_directory）。
试运行核对可读取字节但不保存副本；实际导入将原身份、逐项结果和历史副本引用一起提交。
只复制明确公开的文件，密封/未确认公开项不读取。摘要的源行数与选择数分别报告，不能
以选择清单冒充完整旧库覆盖。副本保存在 state-dir/historical-artifacts；备份/恢复必须
保留该目录与数据库对应关系。重复导入不会创建活动产物或科学资格，原字节冲突拒绝导入。
在网页“设置 → 历史迁移 → 报告 → 历史附件与覆盖情况”查看摘要、逐项结果和分页；
只有本报告已保存的公开副本提供下载按钮。CLI 的 migrate artifact-summary/artifacts/artifact/download
提供相同入口，下载成功后向 stdout 输出完整原字节。
真实旧备份、完整密封沿袭及 T42 恢复验收仍未完成。

旧库移除外键不代表引用已有效。导入会按已支持的0029表关系基线补查可投影引用；
MISSING_DECLARED 仍需核对原库结构。约束改名不影响关系匹配，重复约束不会重复计数。
这项检查不替代多态身份、密封证据沿袭或真实备份恢复验收。

恢复访问切换使用 [CLI recover-access](CLI.md#恢复后的访问切换)。只在API/Worker停止和旧事务
结束后，以迁移所有者连接恢复库执行；非所有者拒绝。保留本次恢复编号与回执以处理未知
结果，新的恢复另取编号。保留对应原生密钥及TOTP密文后重新登录，重新签发所需机器凭据；
外部Provider/Downstream凭据和原生Codex profile按其所有者的恢复流程核对。
备份与归档恢复参数以 [PostgreSQL 18 pg_dump](https://www.postgresql.org/docs/18/app-pgdump.html)
和 [pg_restore](https://www.postgresql.org/docs/18/app-pgrestore.html) 为准；访问切换测试不能证明
数据库、artifacts/historical-artifacts和远端任务已一致恢复。RPO/RTO仍待实际演练记录。

恢复回归测试位于 `apps/server/tests/recovery_access.rs`。在一次性SQLx数据库环境中运行
`cargo test --locked -p server --features native-codex --test recovery_access`；本机PATH需有匹配
服务器版本的原生pg_dump/pg_restore以及GNU tar。CI通过QZ_TEST_PG_CONTAINER指定已有的一次性PGMQ容器，
使用容器内数据库工具；连接凭据只通过环境传递。测试在没有并发写入时，以原生tar归档一次性实例的状态目录（合成TOTP与会话密钥的密文、附件及临时创建的历史文件/profile样例），明确排除master.key；主密钥保存在独立的私有恢复目录。随后生成custom数据库归档，向独立目标库单事务恢复，并从独立恢复输入恢复主密钥。原目录和恢复目录均与tar归档比较，再核对同密钥旧会话、访问切换和原始请求回执。
此测试不读取用户备份或账号profile，也不证明完整历史关系、原生账号/Thread恢复、远端任务对账或生产RPO/RTO。

上述恢复回归现还覆盖一个通过HTTP上传的合成附件：只恢复数据库时下载返回503；
从同一tar归档恢复附件目录后，原元数据和下载字节完全一致。恢复库通过迁移入口重新授予应用角色
权限，随后以独立非所有者登录执行登录、查询和下载；该角色不能运行恢复切换或TRUNCATE。
测试还以同一恢复目录、原生会话密钥及独立应用角色启动真实 `server serve` 进程，通过 TCP 核对旧会话与旧CLI凭据拒绝、新会话查询和附件字节，再停止并回收该进程。恢复切换前复制的CLI凭据可用；切换仅撤销恢复副本中的凭据，原实例仍可用。这证明此受控样例的服务启动、访问与文件恢复，不代表全部历史产物覆盖或完整恢复验收。

磁盘故障的原生回归：Linux 上运行 `cargo test --locked -p integrations --test artifact_publication`。需要 util-linux 的 `unshare`/`mount` 及允许创建用户、挂载命名空间；不可用会失败，不跳过。测试只在子进程私有的 64 KiB tmpfs 内制造 ENOSPC，核对已有产物、失败暂存清理及后续重试，退出后宿主挂载点仍为空。CI 为复制到临时目录的原生 unshare 设置作业专用 AppArmor userns 许可并在结束时移除，不修改全局 userns 策略。这项证据不代替任务准入事务、自动任务、告警或完整 T40 恢复演练。

Cycle 准入的磁盘满回归位于 `apps/server/tests/cycles_http.rs`：在一次性 PostgreSQL/PGMQ 环境运行 `cargo test --locked -p server --features native-codex --test cycles_http`。子进程私有 16 MiB tmpfs 写满后，真实认证 HTTP 启动失败；独立数据库读取确认没有 Cycle/Run/队列残留，旧产物逐字节保留。释放填充文件后，同一请求首次成功且随后重放原回执，只保留一次入队。此项仍不证明所有自动任务、明确存储告警或完整恢复验收。

控制面产物写入遇到原生 ENOSPC 时返回 HTTP 503 / `STORAGE_FULL`，运行日志记录同名错误码。请恢复存储可用空间后使用原请求编号重试，不删除仍被引用的产物。现有回执保持原样；此提示不声称磁盘已恢复或远端任务已停止。

Runtime 的 `cargo test --locked -p runtime --test journal` 另以私有 16 MiB tmpfs 验证真实 `SQLITE_FULL`（原生错误码 13）：新任务和输入写入失败，原任务仍可重放；释放空间并重新打开 SQLite 后，原实例/对象/任务保留，失败的新记录不存在，原新请求可重试且只创建一次。SQLx 连接回池前处理完队列，仍有事务状态的连接会关闭，避免磁盘满自动回滚后污染后续请求。该测试不运行 OCI 科学任务，也不代替完整服务恢复与 RPO/RTO 演练。

Runtime HTTP 上传在真实 SQLITE_FULL/ENOSPC 下返回 503 / `RUNTIME_STORAGE_FULL`、可重试标记与安全错误码日志；其他存储不可用仍使用 `RUNTIME_STORAGE_UNAVAILABLE`。`cargo test --locked -p runtime --test http` 包含私有 tmpfs 的真实认证上传失败、无残留对象、空间恢复后同一编号重试与原回执重放；响应和日志不含测试凭据。该测试使用不可用 Docker 地址，不宣称已运行科学任务。

自动 Forward 的 `cargo test --locked -p server --features native-codex --test forward_automation` 在私有 tmpfs 磁盘满后检查运行、产物元数据、输入、队列、评估和 Handoff 数量不变，并逐字节核对原产物。失败保留正常的 30 秒调度间隔，测试等待真实时钟后再验证并发仅创建一次 Forward 运行，以及取消、丢失 ACK 后原结果重放。它使用受控历史关系，不证明生产 Claim、真实市场反馈或全部自动任务的磁盘故障验收。

恢复回归还在 pg_dump 完成后向原实例写入第二个项目，确认它保留在原库且不进入恢复副本。运行 `cargo test --locked -p server --features native-codex --test recovery_access -- --nocapture` 可获得备份开始/结束、备份后写入、恢复完成的数据库时钟及 `fixture_restore_elapsed_ms`。耗时仅覆盖隔离库恢复、归档附件恢复与目录比较、访问切换、单个HTTP产物核对及测试服务进程启动，不包含生产停机、真实全量文件/profile、远端对账或生产服务重新上线，不能用来宣布生产 RPO/RTO 达标。

交付页的“观察与唤醒”显示原观察分类、原因、Release/Evaluation/政策引用及 Wake 的状态、原因、最早尝试时间和原周期引用；可分别分页、刷新。历史分类不是当前资格，CONSUMED 不表示周期仍在运行，查看不触发工作。
