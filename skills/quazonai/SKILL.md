---
name: quazonai
description: Read the QuaZonai contract and run currently implemented native verification commands.
---
Read ../../DESIGN.md and ../../AGENTS.md before changes. Actual commands are in ../../CLI.md and ../../README.md. The workspace is under rewrite: do not use deleted Python/legacy commands, invent production API endpoints, or mark synthetic native probes as qualified evidence. GitHub Codex is review-only. No approval, downstream control, database/Secret/sealed access is granted to an Agent by this skill.

机器 Bearer 校验出现429时遵守 Retry-After，不用并发重试占用计算槽；不要索取或执行本机 SecretVault 回收命令。人工授权的准确重试只读取原回执，不延长授权或重新消费TOTP。

研究输入和评估政策的真实 HTTP 入口在 CLI「已实现的研究准备 HTTP 合同」：
只读 Agent 必须使用精确项目 RESEARCH_READ，分页时保留 UUID cursor 和 bigint
字符串。输入创建与政策发布需要人工 Operator 授权，技能本身不授予它。
读取 InputSet/Policy 元数据不允许读取 Sealed 原始数据或原生存储位置。
FIXTURE、PIT_UNVERIFIED、未核验方法和政策登记成功均不是 PASS，不触发交付。
新政策分别冻结metric_requirements与sealed_metric_requirements，不能复制分折要求
冒充封存阈值；历史null不补写，需人工新建完整政策和研究周期。
冻结/启动检查Sealed的`asset:N`与登记资产/bar顺序；不允许折scope或借元数据检查读取封存市场行。
EVALUATE_SEALED_ALPHA是内部受管操作，不是Agent工具；不得自行提交任务、读取校准或封存报告来冒充独立评估。
Sealed机会绑定原Attempt并按根血缘累计，失败和取消不退款；不得换UUID或请求补写历史预约。
可信Worker须在ACK前发表原封存评估及全部指标；Agent不得调用发布器、读取报告或把源Validation的PASS当作封存结果。
人工alpha evaluate需精确Alpha目标的Operator授权和运行中Cycle冻结上下文/预算；Mission不能调用或借用该授权。参数不接收模型/校准/原生路径，202不授资格。
不要为尚未接通的可信数据登记/Brief冻结/Worker 路径编造成功结果或使用 SQL 后门。


### 研究产物

POST /api/v2/artifacts 仅在凭据明确具有项目 ARTIFACT_SUBMIT 时使用，提交
schema_version=1、project_id、kind=CODE/PARAMETERS/REPORT、content原文及
Idempotency-Key；文本最多2 MiB UTF-8，JSON文档必须含整数schema_version=1。
不提交路径、origin、producer或任意Run/Attempt，不把报告自报PASS当证据。
相同键的重试必须保留完全相同字节；409不能通过改UUID洗掉试验或输出配额。
BUDGET_EXHAUSTED为HTTP429且retryable=false，不自动重试或换Attempt；安全资源标记
在field_errors中。它不同于带Retry-After的AUTH_RATE_LIMITED，不能把额度不足当临时限流。
读取元数据/内容需RESEARCH_READ，EVALUATOR_ONLY在这些普通接口不可见。
Mission凭据绑定签发时Attempt；过期/撤销/旧Attempt拒绝后交由可信任务服务对账，
不能自行签发身份、请求Operator授权或直连数据库。SYNTHETIC研究提交不是REAL
评估或PACKAGE。完整字段及原生下载行为见CLI「研究产物」。

### 已实现的 Mission MCP

可信任务启动器可运行 `cargo run --locked -p server -- mcp`，精确参数见 CLI。
只通过环境 QUAZONAI_MCP_TOKEN 传入已签发的 Mission 能力；不要复制浏览器会话、
Provider 凭据或数据库配置，也不要自行启动带更广身份/不同绑定的服务。
当前原生 tools/list 包含 `research.get_brief {brief_id}`、`run.get {run_id}`、
`artifact.submit` 和 `experiment.propose`。前两项只返回精确冻结 Brief/本 Mission Run。
`artifact.submit` 需要 ARTIFACT_SUBMIT 和可信启动器的 --workspace-root；参数只有
schema_version、CODE/PARAMETERS/REPORT、workspace_relative_path、idempotency_key，
不能指定根目录或自行带入秘密。文件最多2MiB、普通单链接UTF-8，隐藏路径与软链拒绝。
`experiment.propose` 需要 EXPERIMENT_SUBMIT，参数为 idempotency_key 与完整
ExperimentProposalV1，Cycle 必须等于本 Mission；返回 PENDING 不是科学运行或资格。
Wasm提案的参数格式和范围见CLI「研究产物」：必须明确冻结Discovery版本、固定bars
horizon及原生预测参数，不填MODEL ID或路径；模型只能由可信服务绑定原编译生产者。
首阶段编译已占一次实验，失败/取消也不退款；同一实验的后续预测不重复占次，
但仍占原生资源预算。不得绕过原编译账目或修改历史计数来继续。
可信Worker在原Thread回送编译/Discovery失败或已发表的正式Validation结果；预测
成功直接继续验证，不为中间抽样另开Turn。正式反馈只含已封口元数据及冻结选择
指标，保留来源/口径/有效期，不读取受限报告，也不是资格或排名。失败不猜测详细
编译诊断；修复提案引用原parent_experiment_id，
不覆盖旧输入、不抹去失败。回送独立计入原Mission Turn/修复预算，不自行轮询或重开Thread。
原生公开回答可由可信Worker保存为qz.mission_summary；只含公开text及精确Turn/item/
phase，不收集隐藏推理。它不赋予报告中的PASS或审批文字任何科学/操作权限。
可信Worker在全部用量/科学终态/反馈回答对账后可收束Mission并归档消息；这只是
会话执行结果，不是Cycle完成、实验SUPPORTED、评估PASS或Alpha资格，Agent不能自批。
可信Worker登记的RESEARCH Alpha版本仍未授资格；signal单位/horizon沿用冻结Brief，
不得把SCORE直接作为预期收益或补造calibration。不能用版本存在代替正式评估。
Alpha/正式Validation的完整只读HTTP/CLI操作面仅供Operator及精确项目CLI身份；
Mission不借用它读取额外指标。原Thread反馈仍只披露冻结选择指标与允许元数据，
不读取报告字节或把未过期/科学PASS解释成资格。
`job validate-alpha`是可信本地数值入口，不是Mission可执行工具；其全部折/标签/
训练索引按输入数据权限保留，不直接作为LLM工具输出或手工冒充Evaluation/资格。
`job evaluate-sealed-alpha`同样只属可信本机数值入口，不授Sealed读取或预约权限，
不得由Mission运行或把保留原始分数/标签的报告转交LLM。
受管`VALIDATE_ALPHA`仅接受VALIDATION目录、原MODEL/PARAMETERS；同一原生切分器
在执行与结果采纳侧核对全部折，不允许手工改索引/漏折或借旧镜像声明新能力。
该受管操作仍非Agent自授Evaluation/Qualification入口。
提交响应未知时保留同一 key 和原始文件/请求重放；不同内容409不能改键绕过预算。
未知工具不是可由任意 HTTP/Shell/SQL 替代的能力。保留 UUIDv7 和十进制版本字符串。
每次调用会重新检查到期、撤销及 Attempt 接管，失败不能靠更换 ID、扩大权限或
循环重试绕过。stdout 是协议，不打印解释或 token；任务断开不等于远端 Run 取消。
该入口尚不实现完整发证、原生 Codex 循环和其余研究工具；不得用协议测试冒充生产验收。

### Run 事件与失效授权

Run取消需要近期Operator认证，或精确授权的CLI/AUTOMATION机器权限；研究Mission
不得取消别人的任务。SSE保存最后的run UUID/十进制seq；兼容未知事件只保留公开
envelope并推进cursor，不猜测业务状态。不兼容主版本应升级，不能跳过来伪造连续流。
InputSet冻结不是永久许可，后续任务或首次发送可能因撤销/到期被拒绝。未知远端结果
继续对账，不擅自重发或要求清账。无Cycle管理准入是内部服务能力，不是Agent工具。

Brief草稿详情和列表已接通；创建/完整替换仍是Operator命令，不能凭RESEARCH_READ修改预算、数据角色或政策。人工CLI创建授权还绑定路径project_id和schema_version，更新绑定精确Brief及expected_revision。DRAFT保存不是freeze/PASS，FROZEN只可新建版本。

冻结/启动还核对原生Validation方法与登记元数据。能力拒绝时保留原政策，按DESIGN
A4.6处理准确scope/版本/单位/周期或不支持的输入组合；不得替用户改指标以求通过。

正式验证准入遵循A4.7：保留原试验/MODEL和冻结镜像，不把内部入队当已发表评估，
不以普通研究产物读取方式暴露EVALUATOR_ONLY报告。
原生Worker按A4.8在ACK前发表正式评估，失败/取消和缺证据不授PASS；Agent不能
调用内部发布器、手填指标或把qz.alpha_evaluation报告存在当作资格/Reviewer批准。
Mission须等待正式验证、评估发表和原Thread反馈回答；不能自行宣布跳过这些步骤。
Reviewer的原生请求/总结由可信服务按原角色保存为EVALUATOR_ONLY；不得借普通
研究上传或读取接口转交审阅材料，也不能把任意封存参数作为模型Turn请求。
封存机会拒绝由可信未发送结算器处理，不能冒充Runtime失败、退款或已停止远端任务。
上述是成功收束条件。人工取消/真实到期后由可信Worker停止新阶段，保留已启动
任务和完整用量对账；只有账本证明未发送才记NOT_SENT。Agent不能自行退款、补零、
提交取消成功或删除未发表的评估消息。
原Mission确认前由可信事务冻结A4.9全Family试验选择，保留失败/未完成及原指标；
快照失败保留原队列重试，不新建Run或模型轮次。Operator/精确项目CLI使用cycle
selection/trials只读命令；Mission不能调用完整快照接口、刷新旧排名或把COMPLETE
当作科学PASS、Reviewer批准或资格。未形成与空快照不同。

可信Validation发布器按A4.4冻结原生最后折的SCORE校准、真实训练截止和原输入
子集；Agent不能提交系数、选择赢家折、重写旧Alpha版本或把校准存在当作资格。
无可用拟合不回退，文件/事务失败保留原Run重试，不重新运行模型或读取Sealed。
附加校准创建同Alpha的新不可变版本，原实验/账本/源评估不变；不能把源版本PASS
当作新版本资格。人工alpha calibration只读元数据/源Validation，不向Mission开放。

人工Cycle启动除冻结Brief和Project版本外，还须明确researcher_profile/reviewer_profile的profile_id及expected_revision；不得默认第一个账号。选择随Cycle封口，旧Cycle不跟随后来Profile修改。该命令不授予Mission选择账号、修改模型设置或发起账号操作的权限。

Codex账号操作仅供人工设置页或精确Operator grant的CLI使用，不是Mission MCP工具。
登录、注销、取消与只读状态命令见CLI；模型不能索取设备码、Token、auth.json或账号密码。
202只表示接受人工操作，UNKNOWN和等待截止不能宣称取消成功；操作结束后须重新探测。
