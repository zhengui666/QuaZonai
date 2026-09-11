# QuaZonai Agent 治理

本文件只定义开发治理与导航，不复制产品状态机。产品、领域、接口、数据、安全、运维和完整验收合同统一在 `DESIGN.md`。

## 事实源与顺序

1. `DESIGN.md`：唯一完整的产品与架构事实源，包含字段级附录 A、接口/状态机/测试附录 B，以及所有者对 #62 的语言与复用修订。
2. `OPERATIONS.md`：用户运行说明；`CLI.md`：命令和原生协议的实现展开；二者不得另创产品事实。
3. `skills/quazonai/SKILL.md`：薄工作流、真实命令和权限边界，不是另一套业务引擎。
4. `README.md`：入口、当前实现状态、可执行启动和文档索引。
5. 代码、测试、CI/Review、`docs/architecture/issue-62-execution.md` 和兼容性矩阵：可核验实现证据，不得把目标写成已交付。

外部 Issue/评论是需求出处，不是随时可变的架构依赖。需求变更先进入 DESIGN，再改代码。旧实现只存在于 Git 历史，不在活动源码树保留兼容层或归档代码。用户数据、备份和许可证不属于可删除旧代码。

## 所有者修订与实现纪律

- 第一方目录不得以 `qz-` 开头；直接使用 `apps/server`、`apps/runtime`、`apps/job`、`crates/contracts`、`crates/domain`、`crates/store`。
- 旧代码无需兼容或保留；删除旧服务、旧前端、重复引擎、旧专属测试与部署配置，不能移动到 legacy/archives 伪装删除。保留用户数据、Git 历史、LICENSE/NOTICE。
- 能复用 Rust 组件的能力必须采用 Rust；先查目标版本的真实 API、特性、成熟性、许可和运行结果。不能用本机缺工具链、一次编译错误或语言占比作为选 Python 的理由。
- Python 仅限已提交 `docs/research/reuse.md` 的具体例外：必须列出所需能力、核查的 Rust 候选与具体缺口、上游证据、锁定版本、薄适配和隔离边界及退出条件；不需要再次问用户。不能泛称“Rust 生态不成熟”。
- 只实现 QZ 独有规则、权限、关联和最小适配；不重建数值优化、回测撮合、Agent 工具循环、OAuth 刷新、消息投递、认证算法或容器平台。
- 使用 Ponytail 原则：删除无真实需求的抽象；平台原生能力优先；旧错误路径不加永久兼容 wrapper；不为每张表建立服务、Repository/Factory 或通用 Workflow DSL。
- 前端仍必须使用 React/TypeScript/官方 Ant Design；语言修订不改变这项要求。

## 不可越过的边界

- QZ 是研究与 target-only 交付系统，不拥有 Broker/Exchange 凭据、真实订单/成交/仓位/账户/NAV、下游执行风控或启停/撤单/平仓权限。
- Agent 不拥有 Operator/Reviewer/Downstream 身份，不能审批、交付、改政策、写数据库、读 Sealed raw data、Secret 或任意 URL/宿主路径。
- 原生 Codex App Server 管理模型会话、工具循环和认证；任务有界、Thread 持久、Reviewer 独立；不复制 canonical 聊天数据库。
- 不读取、索取、保存或展示模型隐藏 chain-of-thought。审计只记录可观察调用、变更、真实结果、公开总结和领域事件。
- 不可变版本、审批、Package、试验账本和证据暴露不得原地改写或因复制 UUID 清零；取消不能假称远端已停止。
- 不新增应用级 SHA/hash/checksum/digest/fingerprint 身份或业务门禁。Git、OCI、wheel、存储、成熟密码学组件的原生完整性机制不受此禁令影响，但不能冒充领域资格。
- 不销毁用户旧数据、擅自变更 LICENSE/NOTICE、将 Demo 变成生产可交付证据，或向未审查 PR 代码提供生产秘密。

## 开发接管（所有者修订，2026-09-11）

- 所有者明确授权当前本机 Codex 会话接管 #62/#63 的全部源码、测试、配置、脚本、文档、验证与 Git/GitHub 操作，取代网页唯一作者及本机仅执行 handoff 的限制。当前会话自行修复，不再等待网页作者；不另行启动模型来绕过这一职责。
- 使用 Ponytail Skill，优先复用现有实现。编辑、验证、发布串行；每次源码变化重新验证，原生生成物由真实 Rust 合同及生成器产生，不手改生成代码。
- 这是个人项目。保护账户密码、钱包、支付/API 凭证等敏感信息不进入 LLM、源码或日志；不新增其他网络安全专项或额外安全审查门禁。业务正确性、不可变证据、审批和用户数据保护仍须满足 DESIGN。
- 保留现有有效修改；核实并停止冲突的开发进程后接管。用户数据、凭据、已有数据库、生产服务和无关进程不属于清理目标。开发执行器配置不进入产品部署。
- GitHub Codex 仅用于 review。全部开发完成后持续处理最新 Head 的 review，所有适用 CI 通过且明确无问题后直接合并 main，无需再次请求合并授权。

## 工作顺序与验证

读取 DESIGN 对应章节 → 确认 ownership/data flow → 必要时先更新 DESIGN → 同步用户文档/CLI/Skill → 最小正确实现 → 最窄有效验证 → 跨边界验证 → 独立 review → 汇总已验证与未验证项。

每项检查说明要发现的失败及失败后的行动。不能仅用 mock 证明原生 Codex/MCP/Thread resume、PGMQ/数据库并发、Sealed/Secret/文件系统隔离、原生科学数值、Package/Claim 竞态、SSE 恢复或备份迁移。具体 T01–T42 与检查族完整定义在 DESIGN 附录 B。

## PR #63 / Issue #62 完成边界

同一 PR 完成全部工作包和合同 → 最新 Head 的全部适用 CI 通过、所有 Review Thread 解决且 `@codex review` 明确无问题 → 才允许 merged → main 检查及迁移/完整链路/隔离/恢复/文档证据复核 → 才关闭 #62。

Head 变化必须重新验证。缺失、失败、取消、应执行却跳过、额度不足、未回复或仅 emoji 均不是通过。局部 W0、文档更新、创建 PR、页面壳或大量 mock 不能替代完成。

**GitHub 上 Codex 只用于 review；禁止要求其修复、实现、提交或自动处理问题。本机负责人编写修复与测试，push 后针对新 Head 重新请求 review。** 未满足合同只能报告部分完成，不能合并骨架或宣称 release-ready。
