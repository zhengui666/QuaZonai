---
name: quazonai
description: 通过 quazonai CLI 操作 QuaZonai 量化研究工作台，将用户目标转成可执行命令并读取 JSON 回执。用于查询数据和模型、提交或恢复研究、跟踪任务、分析因子与组合、回放 tick、管理 PM mandate/campaign/strategy、交付目标、观测 paper/live 账户、处理资金退出，以及查看用量、设置、扩展和维护状态。适用于代用户使用已安装产品的 Agent；不用于修改项目源码、实现功能或部署后端。
---

# Quazonai

把用户目标转成原生 `quazonai` 命令，保存请求与回执，再按真实业务状态交付结果。使用 CLI 提供的功能，不直接读写服务数据库或自行拼装另一套 HTTP 客户端。

## 在线安装与更新

在项目目录运行以下命令，即可从 GitHub `dev` 分支安装此技能到 Codex：

```sh
npx skills add https://github.com/zhengui666/QuaZonai/tree/dev/.agents/skills/quazonai --agent codex
```

更新到 `dev` 的最新版本：

```sh
npx skills update quazonai
```

安装后在新对话中调用 `$quazonai`。仅当前项目要使用时，在该项目目录安装；需跨项目使用时添加 `--global`。

## 1. 连接并识别能力

先运行：

```sh
quazonai --version
quazonai --help
quazonai identity
```

复用当前登录。缺少 CLI、需要首次登录、切换实例或出现证书问题时，读取 [连接、输出与错误](references/connection-and-errors.md)。首次登录必须由用户在自己的交互终端输入密码。

按实际安装版本的 `--help` 和 `schema` 选择命令。不要添加 `--json`、`--yes`、`--api-key`、`--password`、`--profile` 或任意 HTTP 请求子命令：当前 CLI 没有这些入口。

## 2. 选择最短操作路径

| 用户目标 | 先读取 | 操作参考 |
| --- | --- | --- |
| 研究一个想法、查进度、恢复研究 | `datasets`、`research models`；已有任务用 `research get ID` | [研究、因子与组合](references/research-and-portfolios.md) |
| 筛选因子、组合回测、容量分析、tick 经济回放 | `factor list`、`factor get ID`、`datasets` | [研究、因子与组合](references/research-and-portfolios.md) |
| 管理持续投资目标、研究活动或策略 | `pm list`、`pm get ID`、`pm capabilities` | [PM 操作](references/pm.md) |
| 绑定模拟/实盘账户、目标交付、自动配置或资金退出 | 对应组合/mandate、`observation list`、`capital status ID` | [账户与资金](references/accounts.md) |
| 查用量、设置、扩展或维护模式 | 对应 `get`、`list`、`status` | [命令索引](references/commands.md) |

单独的研究不要求先创建 PM、组合或账户观测。目标已有关联资源时，使用原 ID 继续，不重新提交同一项工作。

## 3. 形成请求

1. 从实际列表、详情或回执取得数据集、模型、因子和业务 ID。保留原 ID；仅研究 `conversationId`、`submissionId` 使用原生安全整数，其余 ID 按不透明字符串传递。
2. 对文件请求先查 `quazonai schema list`，再查 `quazonai schema show NAME`。原生科学合同使用 `--domain`，不要混用两套名称。
3. 普通 `schema show` 返回 `{format:"typescript", entry, source}`，其中 `source` 包含多份合同；查找目标类型及依赖，不把整个输出作为请求。原生 `--domain` 返回 JSON Schema 及引用组件。Schema 是结构说明，不验证账户权限、数据可用性或科学结论。
4. 将用户目标、真实资源和已明确的参数写成一个 JSON 对象文件。仅在必需的目标、资金、连接或授权范围无法从当前结果确定时补问；不要编造模型、连接、金额或约束。
5. 为需要 `requestId` 的新操作生成一次唯一字符串并保存。复用原请求时同时保留原文件内容、`requestId`、资源 ID 和 `expectedRevision`。存在 `--request-id` 时，它必须与文件中的值一致。
6. 将金额、权重、纳秒和十进制字段保留为字符串，例如 `"10000"`、`"0.25"`。保留 `null` 与缺失值；不要把不可用指标改成零。

使用 JSON 序列化器写文件，使用参数数组或正确的 shell 引号传入文本和路径。不要将用户文本拼成 shell 代码。大请求优先用 `--file`，不要把请求塞入不存在的内联 JSON 参数。

## 4. 提交一次并观察原对象

```sh
quazonai research start --file research-request.json
quazonai research wait "$research_id" --timeout 60
quazonai research get "$research_id"
```

从第一条命令的回执取得 `research_id`，不要照抄示例 ID。把 stdout JSON、stderr 和退出码分开保存。

- 普通业务成功输出通常是一行 JSON；`watch` 输出多行状态 JSON，最后另有总结。`--help`、`--version` 是文本。
- `watch` / `wait` 不带 `--timeout` 会持续等待。需要一次有限观察时显式设置秒数；超时或中断只分离客户端，不取消远端工作。
- 研究以原 submission 的 `done` / `unanswered` 判定结束。`done` 仍需检查 `final`、`criterion`、指标及错误，不能等同于科学目标达成。
- 组合以 `COMPUTED` / `FAILED` / `STOPPED` 判定结束。退出码 `0` 本身不能证明正在观察的对象已经完成。
- 写入出现 `write_outcome_unknown_retain_original_request` 时，先查询原对象或列表。没有回执时保留未知状态；仅在确需显式重放时发送原命令和原文件，不换新 ID 重建工作。
- 出现修订冲突时读取当前详情，判断原意是否仍适用；若决定发起新操作，保存新请求并使用新的 `requestId`。不要在未知结果的原请求上偷偷换修订号。

详细退出码与恢复步骤见 [连接、输出与错误](references/connection-and-errors.md)。

## 5. 按实际业务含义收尾

报告所用实例、资源 ID、当前状态、关键结果、阻塞原因，以及请求/回执文件位置。未完成时附下一条读取原对象的命令。

区分研究结果、组合结果、目标回执和账户事实。paper/live 观测绑定已有账户；目标接收不代表成交，研究完成不代表目标已交付，资金退出命令被接收不代表资金已到账。

仅在用户操作范围内执行停止、删除、目标交付和账户控制。`delete` 隐藏符合条件的非活动对象并保留证据；它不停止远端工作、不清仓。遇到活动状态或依赖阻塞，报告原因，不自行追加停止或删除关联资源。
