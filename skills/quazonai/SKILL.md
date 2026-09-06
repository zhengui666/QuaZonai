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
不要为尚未接通的可信数据登记/Brief/Worker 路径编造成功结果或使用 SQL 后门。


### Run 事件与失效授权

Run取消需要近期Operator认证，或精确授权的CLI/AUTOMATION机器权限；研究Mission
不得取消别人的任务。SSE保存最后的run UUID/十进制seq；兼容未知事件只保留公开
envelope并推进cursor，不猜测业务状态。不兼容主版本应升级，不能跳过来伪造连续流。
InputSet冻结不是永久许可，后续任务或首次发送可能因撤销/到期被拒绝。未知远端结果
继续对账，不擅自重发或要求清账。无Cycle管理准入是内部服务能力，不是Agent工具。
