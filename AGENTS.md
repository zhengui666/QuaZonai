# QuaZonai

## 定位

单人本机量化研究与 target-only 交付。后端 Rust，前端 React / TypeScript / 官方 Ant Design。复用 Codex、NautilusTrader、Clarabel、Arrow 和 PostgreSQL/PGMQ 的原生能力。

## 按任务定位

| 改动 | 入口 |
| --- | --- |
| 领域、API、数据、组合、Worker、Runtime | [.opensdlc/architecture.md](.opensdlc/architecture.md) 的对应章节与源码链接 |
| 开发环境、合同生成、检查命令 | [.opensdlc/project.md](.opensdlc/project.md#commands) |
| 镜像发布、运行故障与恢复 | [.opensdlc/operations.md](.opensdlc/operations.md) |
| 用户安装与更新 | [部署包手册](deploy/docker/README.md) |
| 操作已有服务 | [服务 Skill](skills/quazonai/SKILL.md)，不是开发工作流 |

## 实现边界

Wire 类型属于 `contracts`，纯规则属于 `domain`，事务属于 `store`，传输与编排属于应用入口。依赖方向见[架构](.opensdlc/architecture.md#modules)。生成文件从原始合同生成，不手改 `contracts/generated/` 或 `apps/web/src/generated/`。

保留冻结输入、PIT、Sealed 隔离、独立评估、试验账本、预算、回执和防重复执行。缺失数据不补零，计算成功不等于证据通过；本地进程退出不等于远端取消。QZ 不持有券商账户、不发送真实订单。

保留用户数据、迁移、备份、许可证和无关改动。凭据不进入源码、日志或模型上下文。不增加无实际用途的抽象、多用户管理或安全门禁；不以演示数据或假成功代替真实行为。

## 文件与执行

任务入口为 `.opensdlc/tasks/<task-id>/task.md`；共享合同维护在 `.opensdlc`，历史由 Git 保留。

## Code Review Rules

按 [.opensdlc/review.md](.opensdlc/review.md) 检查改变的行为、接口与恢复边界。GitHub Codex 只读 review。最终 Head 的适用 CI 全部通过、review 问题解决且 Codex 明确无问题后才能合并；未执行、旧 Head 或仅写入交接配置不算验证通过。
