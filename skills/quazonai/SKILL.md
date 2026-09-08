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
不要为尚未接通的可信数据登记/Brief冻结/Worker 路径编造成功结果或使用 SQL 后门。


### 研究产物

POST /api/v2/artifacts 仅在凭据明确具有项目 ARTIFACT_SUBMIT 时使用，提交
schema_version=1、project_id、kind=CODE/PARAMETERS/REPORT、content原文及
Idempotency-Key；文本最多2 MiB UTF-8，JSON文档必须含整数schema_version=1。
不提交路径、origin、producer或任意Run/Attempt，不把报告自报PASS当证据。
相同键的重试必须保留完全相同字节；409不能通过改UUID洗掉试验或输出配额。
读取元数据/内容需RESEARCH_READ，EVALUATOR_ONLY在这些普通接口不可见。
Mission凭据绑定签发时Attempt；过期/撤销/旧Attempt拒绝后交由可信任务服务对账，
不能自行签发身份、请求Operator授权或直连数据库。SYNTHETIC研究提交不是REAL
评估或PACKAGE。完整字段及原生下载行为见CLI「研究产物」。

### 已实现的 Mission MCP

可信任务启动器可运行 `cargo run --locked -p server -- mcp`，精确参数见 CLI。
只通过环境 QUAZONAI_MCP_TOKEN 传入已签发的 Mission 能力；不要复制浏览器会话、
Provider 凭据或数据库配置，也不要自行启动带更广身份/不同绑定的服务。
当前原生 tools/list 只有 `research.get_brief {brief_id}` 和 `run.get {run_id}`。
前者仅返回绑定的冻结版本，后者仅返回该 Mission 的真实 Run/Attempt；未知工具
不是可由任意 HTTP/Shell/SQL 替代的能力。必须保留原生 UUIDv7 和十进制版本字符串。
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
