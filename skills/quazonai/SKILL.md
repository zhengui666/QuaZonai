---
name: quazonai
description: Read the QuaZonai contract and run currently implemented native verification commands.
---
Read ../../DESIGN.md and ../../AGENTS.md before changes. Actual commands are in ../../CLI.md and ../../README.md. The workspace is under rewrite: do not use deleted Python/legacy commands, invent production API endpoints, or mark synthetic native probes as qualified evidence. GitHub Codex is review-only. No approval, downstream control, database/Secret/sealed access is granted to an Agent by this skill.

下游原生能力传输验证：`cargo test --locked -p server --test downstream_transport`。
固定target-only合同和真实TCP边界、不可变观察及downstream probe/readiness HTTP/CLI
已接通；原生PG验证 `cargo test --locked -p store --test downstream`，完整人工grant/
HTTP/TCP/文件链 `cargo test --locked -p server --test downstream_http`。独立部署
DOWNSTREAM_TARGETS默认拒绝，精确配置版本/60秒有效期不可由重放刷新。人工审批/Offer/Claim已消费该观察；完整交付链仍待
验收，不把协议fixture当生产下游或交付验收。

组合指标薄适配只消费原Nautilus Returns组；日均收益不年化，波动率/Sharpe保留
252日原生约定。缺值不填零，不从canonical position收益回退；该映射不授予
Evaluation或Release。实际子进程验证使用cargo test -p job --test simulation --locked。
candidate-simulation/2复用DATA_VALIDATE输出原source_selection的qz.data_quality，
另输出实际窗口qz.native_simulation；完整原manifest必须同时绑定两份报告。
portfolio-sequence/1另支持原Candidate目标序列逐项绑定并进入同一个原生账户；
不将该计算入口称为已发表PORTFOLIO/PASS，不手填目标绕过正式准入。
portfolio-study/6执行原模型驱动、固定间隔或原参数手动截止的单账户滚动研究，验证命令为
`cargo test --locked -p job --test study`。portfolio-calendar/2绑定原完整会话文件及
目录原元数据登记的完整会话表并逐值比较，只作原收盘时间加偏移；不能拿默认周历或手动子集冒充完整源。
手动截止不排序、不补点，
原TTL必须覆盖下一截止与评估末尾，固定间隔不允许手动覆盖。
portfolio-rolling-liquidity/1要求原测量政策文件，每截止重新测量本目录前缀，
执行时复核专属年龄；不复用过期BAR快照、不手填成交额或升级DATA_BACKED。
portfolio-build-rolling/1在原生Build使用相同政策及BAR来源校验，保留原测量报告；
Store Build准入冻结原政策输入，Candidate发布重读政策并在文件发布后复核原BAR期限；
过期保留求解结果但无可用目标，不代表正式PORTFOLIO Study或Release资格。
portfolio-history/1输出原manifest绑定的Arrow历史目标；失败帧仍保留null权重。
逐帧原输入、实际模拟及Arrow报告不是正式PORTFOLIO发布或PASS。
Candidate保持研究的可信发布器从原双报告、独立政策与来源复核发表FORWARD
Evaluation，封口后才ACK；不授Release或下游权限。成功进程不等于指标PASS。

机器 Bearer 校验出现429时遵守 Retry-After，不用并发重试占用计算槽；不要索取或执行本机 SecretVault 回收命令。人工授权的准确重试只读取原回执，不延长授权或重新消费TOTP。

研究输入和评估政策的真实 HTTP 入口在 CLI「已实现的研究准备 HTTP 合同」：
只读 Agent 必须使用精确项目 RESEARCH_READ，分页时保留 UUID cursor 和 bigint
字符串。输入创建与政策发布需要人工 Operator 授权，技能本身不授予它。
读取 InputSet/Policy 元数据不允许读取 Sealed 原始数据或原生存储位置。
InputPurpose与DataPartition分别核对；PORTFOLIO只含原DISCOVERY/VALIDATION成员，
不能替代旧任务要求的用途或绕过RESEARCH_AND_PAPER许可。
独立Study计划随EvaluationPolicy.portfolio_study_plan冻结原输入/起点/手动时点；
没有计划不能正式Study，不能在Run请求临时改窗口；保存不等于运行或PASS。
Store.start_portfolio_study复用原Operator授权、预算和PGMQ，核对原完整成员与
包含Sealed选择观测的研究可用截止；只向原生任务提供唯一PORTFOLIO目录。
可信Worker发表独立PORTFOLIO结果，复核原三报告及Arrow历史；取消/失败/不可行
不授PASS，原发表回执才能ACK。现有Candidate评估读取保留原类型，不借HOLD。
正式入口为client portfolio study及POST /api/v2/portfolio-studies，仅六字段意图，
CLI grant使用PORTFOLIO_STUDY命令，沿用PORTFOLIO_SIMULATE权限；不表示完成组合交付。
候选详情可显式请求同一Study，原政策只读；未知结果保持原意图和幂等键重试。
FIXTURE、PIT_UNVERIFIED、未核验方法和政策登记成功均不是 PASS，不触发交付。
新政策分别冻结metric_requirements与sealed_metric_requirements，不能复制分折要求
冒充封存阈值；历史null不补写，需人工新建完整政策和研究周期。
冻结/启动检查Sealed的`asset:N`与登记资产/bar顺序；不允许折scope或借元数据检查读取封存市场行。
portfolio_metric_requirements是独立组合条件；null不能授予组合PASS，不能复制
Alpha/Sealed阈值或原地补写历史政策。保存条件不是原生指标/Evaluation验收。
EVALUATE_SEALED_ALPHA是内部受管操作，不是Agent工具；不得自行提交任务、读取校准或封存报告来冒充独立评估。
Sealed机会绑定原Attempt并按根血缘累计，失败和取消不退款；不得换UUID或请求补写历史预约。
可信Worker须在ACK前发表原封存评估及全部指标；Agent不得调用发布器、读取报告或把源Validation的PASS当作封存结果。
人工alpha evaluate需精确Alpha目标的Operator授权和运行中Cycle冻结上下文/预算；Mission不能调用或借用该授权。参数不接收模型/校准/原生路径，202不授资格。
可信数据登记、Brief冻结和Worker已有原生路径；只有原接口回执与实际运行证据才能确认结果，不得编造成功或使用SQL后门。


### 交付人工决定

release approve / approval show/list见CLI；list按原Release分页所有历史审批，不授当前交付资格；人工grant绑定精确Release和完整审批请求。
服务端同事务冻结原评估报告引用，不允许指定evidence_set_id或复制私有报告为执行输入。
审批绑定下游配置与Candidate决定序号；历史查询不是当前交付授权。handoff offer/show见CLI：精确审批人工grant、当前来源/撤销/readiness重验和最新前版CAS；原Release不能换键再发送。Claim使用精确下游/项目DOWNSTREAM_CLAIM机器凭据，幂等键=external_claim_id，返回原Package和唯一转移；不得借用Operator/Mission身份。Worker只补记未领取Offer的到期/有效撤销，原领取重放不刷新期限。ACK使用精确DOWNSTREAM_ACK及原领取编号，不能改写终态或重授交付。approval revoke需要绑定原审批/完整意图的APPROVAL_REVOKE人工grant；立即或未来撤销均追加，最早生效记录不可推迟。字段和历史分页见CLI，已领取事实保留。
最窄原生PG回归：`cargo test --locked -p server --features native-codex --test portfolio_study_http release_freezes`；
CLI授权回归：`cargo test --locked -p server --features native-codex --test client_portfolio_build release_approval`。

release reject/reconsider/decisions见CLI；写入需要原目标的近期Operator授权。
REOPEN只追加原最新REJECT的后继，不恢复任何审批。Mission不得调用或借用人工grant。
不同Release共享原Candidate/下游/环境的决定历史，不能换UUID绕过拒绝。

### 研究产物

TargetPackageV1不接受订单字段；内部Store.create_release要求精确Candidate的
RELEASE_CREATE人工授权和原PORTFOLIO/PASS，服务端组装，不接受包上传或强制通过。
client release create/show/list及原生HTTP已接通，list按精确项目RESEARCH_READ分页，具体字段见CLI；
浏览器组合候选的独立PORTFOLIO/PASS可确认冻结，交付页只读原Release列表/详情；
未知结果必须保持原请求/键，REAL来源不代表Live审批。
成功创建仍待验收，201仅冻结版本，不得以结构通过替代当前资格或审批。

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
bar-notional/1的DATA_VALIDATE保留非Sealed最后已知BAR的原生收盘估值明细；
null是未测量，Sealed不输出，不把该观察当作未来流动性、DATA_BACKED或组合资格。
受管`VALIDATE_ALPHA`仅接受VALIDATION目录、原MODEL/PARAMETERS；同一原生切分器
在执行与结果采纳侧核对全部折，不允许手工改索引/漏折或借旧镜像声明新能力。
该受管操作仍非Agent自授Evaluation/Qualification入口。
本机allocate保留原forecasts集合；受管PORTFOLIO_BUILD必须从冻结FORWARD目录
和原MODEL产物生成预测/收益，不接受手填数组或将合成Alpha标识当作资格。
portfolio-ensemble/1需真实重建登记镜像；这些命令不增加Mission工具或审批权限。
optimizer/alpha_ensemble须保留原NativeModelRefV1，不能改类名/版本或把未知参数
当默认配置；顶层settings/risk_aversion不再接受，后者在原optimizer.parameters，
对应镜像还须portfolio-models/4；本机allocate以return_history与covariance_estimator
估计协方差，受管任务则绑定selection/mandate/members并输出qz.native_portfolio/1。
SAMPLE_COVARIANCE仅绑定ndarray-stats0.7.0的原生cov与ddof=1，不授予数据访问，
也不新增Mission工具；不能把数值矩阵存在当作原数据资格。
VARIANCE 可冻结正的每决策周期方差上限 max_ex_ante_risk（不用为 null），不是
标准差/年化值；需要 portfolio-variance-bound/1 与 SECOND_ORDER_CONE 能力。
发布以保存后的权重重新估计核对，容差为上限乘 exposure_tolerance；不替换风险度量。
CVAR 必须冻结 optimizer.parameters.cvar_confidence 为(0,1)内Decimal；VARIANCE
时此字段为空。不默认95%，不将尾部场景筛成另一份历史。原生LP使用完整等权
损失场景；此时风险上限为每周期预期损失收益率，不是方差或VaR。需portfolio-cvar/1
与LINEAR_PROGRAM镜像能力；不新增Agent权限。
方差RISK_BUDGETING明确冻结risk_budgeting中的资产身份、非负share、LONG/SHORT及
risky_gross_exposure；share合计1，其他目标为空。需portfolio-risk-budget/1与
SECOND_ORDER_CONE。原生两阶段共用迭代/资源预算，发布核对实际风险贡献，不能把
最小方差、资本等权、全现金或约束后的近似比例当风险预算。
CVaR预算沿用同一份额/方向/总敞口配置及明确置信水平，另需portfolio-cvar-risk-budget/1
与POWER_CONE及原CVAR能力。原生场景对偶见证由可信发布器验证尾部最优性与
每项贡献；不可由Agent填写、挑选场景或归一化。只接受正总风险，失败/无界无目标，
不增加Mission工具、资格或交付权限。
人工Mandate API/CLI创建与读取见CLI，原配置不可修改；同键重试保留原完整请求。
Mission没有这些配置操作权，版本存在不表示Alpha资格、组合通过或允许交付。
提交响应未知时保留同一 key 和原始文件/请求重放；不同内容409不能改键绕过预算。
未知工具不是可由任意 HTTP/Shell/SQL 替代的能力。保留 UUIDv7 和十进制版本字符串。
每次调用会重新检查到期、撤销及 Attempt 接管，失败不能靠更换 ID、扩大权限或
循环重试绕过。stdout 是协议，不打印解释或 token；任务断开不等于远端 Run 取消。
该入口尚不实现完整发证、原生 Codex 循环和其余研究工具；不得用协议测试冒充生产验收。

### Run 事件与失效授权

`client run rebalance RUN_UUID` 读取原自动 Build/Study 的政策、来源 Candidate、原请求及已登记 Study/Release；沿用 RUN_READ 项目/任务范围。空关联不能推断人工来源，历史关联不授当前政策或交付资格。

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
Reviewer的Runtime刷新仍核对原冻结配置、启用状态、租约及期限，不因角色跳过。
独立Reviewer由可信研究ACK事务准入，使用冻结Profile、不同Thread/工作区及原累计预算。
仅审阅可信复制的原CODE/PARAMETERS/Validation上下文，每目标一条原生Turn；不接研究
对话、账号材料、Sealed原始行或校准系数。不获ARTIFACT_SUBMIT/EXPERIMENT_SUBMIT。
按DESIGN B5.0.5返回精确目标JSON；缺证据应INCONCLUSIVE，不自授审批或资格。
无效回答由可信服务保留原总结并记INCONCLUSIVE，不请求额外修复轮或改旧回答。
全部审阅完成后的原PASS目标由可信Worker自动准入Sealed，复用原Cycle预算与
科学准备器，不借用人工alpha evaluate授权、不增加模型轮。原审阅/任务关联齐全
才可成功确认会话；排队不等于科学通过或资格。Agent不得直接调用该内部入口、
补写关联、重置机会或自行处理WAITING_INPUT/预算耗尽状态。
资格仅由原封存ACK按DESIGN A4.14裁决；不得手填、续期、重授已撤销资格，或将
FIXTURE科学PASS冒充REAL资格。当前真实正向授予与完整资格操作面尚未验收。
封存机会拒绝由可信未发送结算器处理，不能冒充Runtime失败、退款或已停止远端任务。
上述是成功收束条件。人工取消/真实到期后由可信Worker停止新阶段，保留已启动
任务和完整用量对账；只有账本证明未发送才记NOT_SENT。Agent不能自行退款、补零、
提交取消成功或删除未发表的评估消息。
取消恢复只由可信Worker重连原Thread对账，不启用MCP、签发凭据或准备新Turn；
清理窗口不是额外研究预算，未知用量仍保持未知。
原Mission确认前由可信事务冻结A4.9全Family试验选择，保留失败/未完成及原指标；
快照失败保留原队列重试，不新建Run或模型轮次。Operator/精确项目CLI使用cycle
selection/trials只读命令；Mission不能调用完整快照接口、刷新旧排名或把COMPLETE
当作科学PASS、Reviewer批准或资格。未形成与空快照不同。
review_alpha_version_id是冻结的原审阅目标，区别于原Validation的alpha_version_id；
SCORE沿原校准确定，不读取active指针推断，REJECT/未入选/缺证据保持空。
目标非空不免除后续Reviewer启动、当前有效期、预算和资格检查。

可信Validation发布器按A4.4冻结原生最后折的SCORE校准、真实训练截止和原输入
子集；Agent不能提交系数、选择赢家折、重写旧Alpha版本或把校准存在当作资格。
无可用拟合不回退，文件/事务失败保留原Run重试，不重新运行模型或读取Sealed。
附加校准创建同Alpha的新不可变版本，原实验/账本/源评估不变；不能把源版本PASS
当作新版本资格。人工alpha calibration只读元数据/源Validation，不向Mission开放。
原生simulate显式冻结fee_model、fill_model、latency_model（nautilus-execution
0.63.0，simulation-models/1），旧顶层insert_latency_ns拒绝，不补默认模型或种子；
模型运行与费用证据、正式执行假设及组合资格是不同边界，不能由成功模拟推定后者。
受管组合当前权重必须提供独立REPORT原产物和PortfolioCurrentWeightsV1冻结副本，
镜像portfolio-weights/1核对来源种类、内容、时间、币种与资产权重，不补NONE为现金。
来源身份与资格仍由Store核验；Agent不得自报下游身份来绕过该边界。
组约束沿原Forward Universe时态成员groups绑定；null/缺省表示未知，[]表示明确
无组。有约束时拒绝未知、非决策时可用、重叠成员及无参与资产的组，Candidate发布
重读原来源；不新增分类引擎或把分类当作数据/费用资格。
`client forward-weights`仅限真实DOWNSTREAM/FORWARD_SUBMIT机器身份登记当前权重，
不是Mission工具或Operator代报入口。保留原external_message_id重试，不以新CLI键
覆盖旧消息；原报告不可变，PAPER为SYNTHETIC，LIVE也不自动获得资格（字段见CLI）。
Operator的`client portfolio assumptions create/list/show`保存/读取原生来源绑定的
不可变假设（字段见CLI）；创建需要冻结非Sealed输入、原登记费率和近期Runtime探测。
当前入口仅保守BAR，不自动获得DATA_BACKED、资格或交付权限；不修改历史假设。
可选bar_liquidity绑定同Runtime/冻结输入/Dataset的原生DATA_VALIDATE报告，
必须明确最大年龄和参与率。目录登记副本不是原生测量；读取原失效时刻不代表
当前可用，到期须新建假设/政策，不延长旧资格。Store准入/发布重读原来源并检查
原期限，需portfolio-liquidity/1镜像；损坏保留重试，到期不授目标，不提升来源等级。
或使用互斥rolling_liquidity冻结每步年龄和参与率，沿用原执行假设事务及文件清理。
需portfolio-rolling-liquidity/1能力；滚动政策为声明参数而非市场量，没有快照失效
时刻。读取原政策文件ID，不授予Study/Build或PASS资格。
Build还需portfolio-cost-source/1及原PARAMETERS费用文档，完整绑定execution_settings；
发布重读保存配置。非零滑点需portfolio-slippage/1、原BAR/tick参考和A5.2规划
系数复核；不是未来成本上界，不二次扣原生模拟净收益，不声明DATA_BACKED。
Forward原目录的资产币种、maker/taker费率也须与原设置匹配；Build/模拟共用原生校验。
SIMULATE_CANDIDATE以原目标REPORT及费用PARAMETERS绑定唯一FORWARD目录，只在
原asof与原Candidate可用时间较晚者起至有效期内保持原目标；需candidate-simulation/2。原生结果不是PASS，
不得当作已接通Store评估或Release，更不能回填历史目标或恢复实际账户。
保持模拟的人工准入意图仅选Candidate、Cycle、Forward输入、Runtime版本和预算，
`client portfolio simulate`及POST /api/v2/candidate-simulations需精确Candidate的
PORTFOLIO_SIMULATE人工授权；不授Mission权限，202只表示Run，不是Evaluation。
原生Build读取已绑定DATA_QUALITY原字节并核对选择、币种、年龄与逐资产量；
这不授Mission写入来源或自行申请资格的权限，也不替代Store准入/发布复核。
浏览器“组合/评估政策”使用已有政策创建/列表/详情API；三组指标独立填写，
组合未定义为null，原版本只读；未知结果保留完整请求及幂等键，不自动启动研究。
`client portfolio candidate list PROJECT_UUID` / `show CANDIDATE_UUID`读取已发布
原始快照；不是当前资格或交付授权，不向Mission开放报告字节。
`client portfolio candidate evaluations CANDIDATE_UUID`分页读取原绑定的已发表评估，
`client evidence show/metrics EVALUATION_UUID`与浏览器候选详情复用同一投影；
缺值不补0、有效期不刷新，不披露Sealed或原报告字节，不自动模拟或授予资格。
`client portfolio build`以原资格、Mandate、Cycle、Forward输入申请Run；
current_weights_source选择下游快照或LAST_TARGET原Candidate目标假设。
CLI需目标Mandate的精确PORTFOLIO_BUILD人工grant。不得手填预测、持仓、费用
或把202称为合格Candidate；正式Candidate须由Worker采纳原生结果并完成发布核对，完整真实数据链及全成本来源仍待验收。
单基础币种CurrencyPair必须显式MARGIN，Equity可用CASH/MARGIN；不改写旧假设。
浏览器在“组合”选择项目后切到“执行假设”，使用同一创建/列表/详情API；未知响应
保留原输入重试，不重新生成费用或种子，也不把关闭编辑器当成撤销。
人工alpha qualifications分页读取原资格及最早撤销（包含未来生效），不读取Sealed
报告/指标。grant_window_open仅核对服务端观察时刻的授予/撤销时间窗，不能替代
当前政策、生命周期、许可证与组合准入检查，不向Mission授予资格或交付权限。

人工Cycle启动除冻结Brief和Project版本外，还须明确researcher_profile/reviewer_profile的profile_id及expected_revision；不得默认第一个账号。选择随Cycle封口，旧Cycle不跟随后来Profile修改。该命令不授予Mission选择账号、修改模型设置或发起账号操作的权限。

Codex账号操作仅供人工设置页或精确Operator grant的CLI使用，不是Mission MCP工具。
登录、注销、取消与只读状态命令见CLI；模型不能索取设备码、Token、auth.json或账号密码。
202只表示接受人工操作，UNKNOWN和等待截止不能宣称取消成功；操作结束后须重新探测。

冻结政策管理见CLI automation authorize/list/show/revoke/revocations及DESIGN A7.0。授权绑定原项目revision与完整指标/范围/期限，撤销绑定原政策与最新撤销CAS。版本不可改写，最早撤销不能推迟；Worker已消费当前有效政策产生原始Paper审批和Offer；登记不保证准入，协议fixture不等于Paper验收或Live交付。

Worker下游刷新使用独立DOWNSTREAM_TARGETS与原DOWNSTREAM vault引用，原生短租约限制同下游并发。只刷新未领取Offer或当前有效自动政策所需观察，固定60秒观察期不延长；失败/过期不授予准入。探测发布与清理共用下游行锁，未知提交保留已引用对象。自动观察刷新不是冻结政策审批消费。

自动 Paper：ACTIVE 项目当前有效 AUTO_PAPER/AUTO_HANDOFF 政策由 Worker 轮询消费，原审批和 Offer 同事务产生。每日限额按数据库 UTC 日、原项目/下游及不同 Candidate 计数，包含人工记录；换政策版本不重置。政策替换、禁用或撤销阻止未领取记录继续领取，已领取事实不改写。`client handoff list PROJECT_UUID --limit 50`（可选 `--cursor UUID`）查询原绑定与当前状态；下游凭据仅见自己的记录。Live 自动晋级已接入同一 Worker，条件与证据边界见下段。

自动 Live：仅当前有效 AUTO_HANDOFF 政策可消费原 Candidate/下游的 Paper 观察。全部已报告原 Paper Handoff/stream 均须有当前原生 HEALTHY 观察，每个流分别满足样本数、完整窗口时长与两组指标；不合并样本或挑选有利流，超过255个流拒绝。审批与Offer同事务冻结完整排序的观察UUID集合；首次Claim重验同一集合、完整来源、Live数据用途、Release与下游readiness。新流、更正、撤权或过期会阻止旧证据继续授权；已有Claim重放保持原事实。同一Candidate当日Paper/Live合计占一次额度。人工Live审批行为不变；浏览器“交付”提供自动化政策、原审批/交付记录及Forward观察历史。完整市场与部署验收仍未完成。

Forward 报告：原 Handoff 领取后，精确项目/下游 FORWARD_SUBMIT 凭据使用 `client --idempotency-key MESSAGE_ID forward submit < forward-message.json` 提交 ForwardMessageSubmitV1（完整字段见 DESIGN A7.3）。external_message_id 必须与请求头/CLI的MESSAGE_ID一致，是原幂等编号，未知结果保持原报告重试；换编号重传相同逻辑消息也只返回原记录。纠正必须引用最新原消息、revision加1并保留窗口。三个时间使用UTC微秒精度；原始收益报告仅保存在EVALUATOR_ONLY Artifact，不能夹带账户/NAV/订单或执行权限字段。`client forward list PROJECT_UUID --limit 50 --cursor UUID`只读元数据；首次省略cursor，下游仅见自己的记录。收到报告不表示连续窗口、统计评估或Live晋级已通过；Worker分别执行原窗口评估和当前政策下的晋级检查，结果以原Evaluation、观察及交付记录为准。


`client forward observations PROJECT_UUID --limit 50` 与 `client forward wakes PROJECT_UUID --limit 50` 只读原观察/唤醒历史；后续页传入返回的 `--cursor UUID`。仅 Operator 或精确项目 RESEARCH_READ CLI 可读；不向 Downstream、Mission 或 Reviewer 开放。历史分类与 CONSUMED 不表示当前资格或周期仍在运行，读取不触发调度。字段及权限见 DESIGN A7.9.1。

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

构建选择原下游权重时可用client forward weights PROJECT_UUID分页读取，需精确
项目RESEARCH_READ；只读原快照，不能把读取当作当前资格或伪造缺失起始权重。


旧快照准备：本机 `server inspect-historical-source --output PATH` 使用受保护的 MIGRATION_SOURCE_DATABASE_URL，只对操作者选定的独立旧副本作原生计数/FK检查；具体前提见CLI。输出新私有文件、不覆盖原报告，不把退出0或零物理孤立行当成完整导入/语义血缘/产物可读证据。Agent/MCP没有源数据库或迁移入口权限。


本机旧产物导出：`cargo run --locked -p server -- export-historical-artifacts --source-root "$MIGRATION_ARTIFACT_ROOT" --selection "$MIGRATION_ARTIFACT_SELECTION" --output "$MIGRATION_ARTIFACT_EXPORT"`。仅部署者审查的私有清单和独立副本，全部绝对路径且输出新目录；公开确认项才复制，密封/未审查项不读取。见 [CLI](../../CLI.md#旧产物的实际字节导出) 的格式、限制和逐项结果；此命令不向 Agent 授予文件/导入权限，也不是全量迁移完成证据。


本机旧行导出：`cargo run --locked -p server -- export-historical-rows --source-installation-id "$MIGRATION_SOURCE_INSTALLATION_ID" --output "$MIGRATION_ROW_EXPORT"`。受保护环境提供独立旧副本的 `MIGRATION_SOURCE_DATABASE_URL`，原安装ID与产物导出保持一致，输出全新绝对目录；参考 [CLI](../../CLI.md#旧库行数据的原生-csv-投影导出) 核对缺表/结构/逐列排除与原/投影行数。原生CSV保留实际被选字段，不等于完整迁移或业务资格，不提供Agent/HTTP路径或数据库能力。

历史投影导入：部署者按 [CLI](../../CLI.md#历史投影注册和导入) 注册冻结原包，Operator 使用 `migrate import --export-ref UUID --dry-run` 或同一 HTTP 合同。实际导入省略 dry-run，需要独立精确授权；CLI 查询只允许本凭据导入报告。Agent 不提供注册路径或源报告，不据只读历史投影宣称完整迁移通过。

历史报告核对：网页“设置 → 历史迁移”，或 CLI `migrate reports/source/mappings`。核对原安装、完整原键、首次导入、缺表和排除项；不得把只读映射和行数当作完整迁移或旧资格继承证据。

历史字段核对：映射行展开字段目录，或 `migrate fields/field` 按next_offset继续。每段16,384字符，保留NULL/空串/Unicode；每段重新鉴权，不读排除字段，不执行旧内容。

历史产物核对：部署者可按 CLI 注册 artifact_directory；报告的 artifacts/summary、分页 artifacts 和 artifacts/{record}/content 为受原报告权限约束的 HTTP 入口。分别核对源行数/选择数/可读数/存储数；仅实际存储且属于本报告的公开副本可下载，dry-run/密封/未选择项不可下载。副本位于独立 historical-artifacts 目录并随数据库备份。网页报告展开“历史附件与覆盖情况”，CLI使用 migrate artifact-summary/artifacts/artifact/download。下载只返回原字节，不执行旧文件；真实恢复验收仍未完成。
