# QuaZonai

## 定位

单人本机、可扩展、无人值守的 AI 研究与 target-only 交付。HTTP、配置、业务编排、CLI、Harness 与 React 前端使用 TypeScript；Rust 只承担远程 Nautilus Worker 的必要科学计算、原生引擎与科学合同／校验；应用不内置或启动 Nautilus，回测与模拟／实盘连接分别配置。复用官方 Pi Durable、pi-ai、Chord、NautilusTrader、Clarabel 和 Arrow，保留 PostgreSQL 历史与迁移。组合与模拟是可选消费者，不是因子研究前置步骤。

## 按任务定位

| 改动 | 入口 |
| --- | --- |
| API、研究、科学计算、策略组合、观测 | [.opensdlc/architecture.md](.opensdlc/architecture.md) 的对应章节与源码链接 |
| 开发环境、合同生成、检查命令 | [.opensdlc/project.md](.opensdlc/project.md#commands) |
| 镜像发布、运行故障与恢复 | [.opensdlc/operations.md](.opensdlc/operations.md) |
| 用户安装与更新 | [部署包手册](deploy/docker/README.md) |
| 操作已有服务 | [服务 Skill](skills/quazonai/SKILL.md)，不是开发工作流 |

## 实现边界

当前 Wire 类型属于 `packages/contracts`，业务事实和编排属于 TypeScript service，Agent 执行状态属于官方 Pi Durable；必要科学类型／纯校验属于 Rust contracts／domain。依赖方向见[架构](.opensdlc/architecture.md#modules)。派生文件从原合同生成，不手改 `contracts/generated/` 或 `apps/web/src/generated/`。

第三方组件只使用官方包或固定的官方上游修订，不在本仓库保存、复制或修改其实现源码，不通过源码补丁或 fork 替换。适配写在 QuaZonai 中并使用公开 API；官方能力不足时明确拒绝不支持的操作，不伪造成功。保留许可证声明及已应用的数据库兼容迁移。

保留冻结输入、PIT、Sealed 隔离、独立评估、试验账本、预算、回执和防重复执行。缺失数据不补零，计算成功不等于证据通过；本地进程退出不等于远端取消。QZ 不持有券商账户、不发送真实订单。

保留用户数据、迁移、备份、许可证和无关改动。凭据不进入源码、日志或模型上下文。不增加无实际用途的抽象、多用户管理或安全门禁；不以演示数据或假成功代替真实行为。

## 文件与执行

任务入口为 `.opensdlc/tasks/<task-id>/task.md`；共享合同维护在 `.opensdlc`，历史由 Git 保留。

## Code Review Rules

按 [.opensdlc/review.md](.opensdlc/review.md) 检查改变的行为、接口与恢复边界。GitHub Codex 只读 review。最终 Head 的适用 CI 全部通过、review 问题解决且 Codex 明确无问题后合并叨远程dev分支，视为完成任务；未执行、旧 Head 或仅写入交接配置不算验证通过。
