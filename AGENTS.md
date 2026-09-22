# QuaZonai Agent 治理

## 事实源与顺序

`DESIGN.md` 定义产品、领域与接口；最新[单人精简修订](DESIGN.md#personal-lean)覆盖旧部署门槛。`OPERATIONS.md` / `CLI.md` 展开实际操作，`docs/architecture.md` 导航源码。`skills/quazonai/SKILL.md` 仅供操作运行中服务的 Agent 使用，不是开发文档。

## 所有者修订与实现纪律

优先复用现有代码、标准库和成熟原生组件，Rust 优先；Python 例外须在 `docs/research/reuse.md` 记录具名缺口。前端使用 React / TypeScript / 官方 Ant Design。不重建数值优化、撮合、Agent Harness、队列、认证算法或容器平台；不新增无实际需求的工厂、服务层、hash 身份或门禁。

删除失去用途的实现及专属测试，不在 legacy/archives 保存副本。Git 保留历史；用户数据、旧迁移、备份和许可证不是清理目标。保留并发开发的有效改动。

## 不可越过的边界

QZ 只研究和交付目标，不操作真实订单或券商账户。研究 Agent 的数据/预算/任务边界和独立评估仍有效，不能用单人模式让它接触 Sealed 标签、凭据或任意宿主路径。冻结版本、账本、回执、恢复和防重复执行保护的是结果正确性，不是多用户管理负担。取消必须以实际远端结果为准；Demo 不得变成真实证据。只记录可观察调用和结果，不索取模型隐藏推理。

## 开发权限

网页 ChatGPT 亲自编写源码、测试、配置、脚本与文档；GitHub Actions 执行原生构建、格式化、生成和验证。不使用 CodexPro / 本机 Codex。GitHub Codex 只做只读 review，不得实现、修复、提交或推送。普通 PR 不使用生产秘密，不操作既有数据库、服务或无关进程。

## 工作顺序与验证

读取相关合同与调用链 → 更新受影响事实源 → 最小实现 → 最窄有效测试及跨边界验证 → 独立 review。一个任务只需 `.opensdlc/tasks/<task-id>/task.md` 记录目标、改动、真实验证和剩余项；不要复制多份计划、日报或批准账本。测试说明它保护的失败模式，不用 mock 替代必要的原生验证。

## 交付与 Issue #62 完成边界

新改动使用新 PR；声明范围完成、当前 Head 的适用 CI 通过、review 线程解决且 `@codex review` 明确无问题后合并。未回复、仅 emoji、取消、缺失和应执行却跳过都不是通过。历史 Head 结果不能证明新代码。维护 PR 不自动关闭 Issue #62；真实账号豁免是 NOT_RUN，其他产品验收见 [DESIGN 0.4](DESIGN.md#acceptance-scope) 和[实现证据](docs/architecture/issue-62-execution.md)。
