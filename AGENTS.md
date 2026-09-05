# QuaZonai Agent 治理

本文件只定义开发治理与导航，不复制产品状态机。产品、领域、接口、数据、安全、运维和完整验收合同统一在 `DESIGN.md`。

## 事实源与顺序

1. `DESIGN.md`：唯一完整的产品与架构事实源，包含字段级附录 A、接口/状态机/测试附录 B，以及所有者对 #62 的语言与复用修订。
2. `OPERATIONS.md`：用户运行说明；`CLI.md`：命令和原生协议的实现展开；二者不得另创产品事实。
3. `skills/quazonai/SKILL.md`：薄工作流、真实命令和权限边界，不是另一套业务引擎。
4. `README.md`：入口、当前实现状态、可执行启动和文档索引。
5. 代码、测试、CI/Review、`docs/architecture/issue-62-execution.md` 和兼容性矩阵：可核验实现证据，不得把目标写成已交付。

外部 Issue/评论是需求出处，不是随时可变的架构依赖。需求变更先进入 DESIGN，再改代码。`docs/architecture/legacy-issue-58-design.md` 只用于旧数据迁移对照，不约束替代架构。

## 所有者修订与实现纪律

- **优先 Rust，其次 Python；优先复用，其次造轮子。** 不是“所有第一方代码必须 Rust”，也不是退回全 Python 的授权。
- 先查仓库、标准库、平台和成熟依赖。功能、安全、维护性适用时优先复用 Rust；成熟 Python 组件比重新写 Rust 内核更合适时复用 Python，记录接口边界、理由、锁定版本和验证。
- Python 不限于第三方库源代码，也可承载必要且有依据的第一方薄适配/服务；不得仅用 Rust 启动器包装整套 Python 来宣称 Rust 优先。
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

## 工作顺序与验证

读取 DESIGN 对应章节 → 确认 ownership/data flow → 必要时先更新 DESIGN → 同步用户文档/CLI/Skill → 最小正确实现 → 最窄有效验证 → 跨边界验证 → 独立 review → 汇总已验证与未验证项。

每项检查说明要发现的失败及失败后的行动。不能仅用 mock 证明原生 Codex/MCP/Thread resume、PGMQ/数据库并发、Sealed/Secret/文件系统隔离、原生科学数值、Package/Claim 竞态、SSE 恢复或备份迁移。具体 T01–T42 与检查族完整定义在 DESIGN 附录 B。

## PR #63 / Issue #62 完成边界

同一 PR 完成全部工作包和合同 → 最新 Head 的全部适用 CI 通过、所有 Review Thread 解决且 `@codex review` 明确无问题 → 才允许 merged → main 检查及迁移/完整链路/隔离/恢复/文档证据复核 → 才关闭 #62。

Head 变化必须重新验证。缺失、失败、取消、应执行却跳过、额度不足、未回复或仅 emoji 均不是通过。局部 W0、文档更新、创建 PR、页面壳或大量 mock 不能替代完成。

**GitHub 上 Codex 只用于 review；禁止要求 Codex 修复、实现、提交或自动处理问题。修复由实际执行者完成并补测，再 push 后重新请求 review。** 未满足合同只能报告部分完成，不能合并骨架或宣称 release-ready。
