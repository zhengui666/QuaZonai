# 命令索引

以下各行省略统一前缀 `quazonai`。`ID`、文件名和 `NAME` 是占位符；仅使用真实回执中的值。命令不支持按资源名称自动查找，先列表再取 ID。

## 会话、合同与查询

| 命令 | 用途 |
| --- | --- |
| `login [--origin URL] [--name NAME] [--replace] [--ca-certificate FILE]` | 交互登录或复用会话 |
| `identity` / `logout` / `forget` | 查身份、撤销当前设备、仅忘记本地登录 |
| `schema list` / `schema show NAME` | 查看业务 TypeScript 合同 |
| `schema list --domain` / `schema show NAME --domain` | 查看原生科学 JSON Schema |
| `datasets` | 查询数据集、ready 状态及研究设置 |
| `research models` | 查询可选模型及推理等级、服务档位 |
| `usage` | 查询用量；`null` 费用或 token 数不是零 |

## 原生研究与历史组合

| 命令 | 请求或注意事项 |
| --- | --- |
| `research list [--cursor JSON]` | 从返回的 `cursor` 对象继续分页 |
| `research get ID` / `research diagnostics ID` | 详情、原 submission 与运行诊断 |
| `research start --file REQUEST.json [--request-id ORIGINAL_ID]` | `ResearchStart` |
| `research start --goal TEXT --dataset ID --model provider:modelId --request-id ORIGINAL_ID` | 简单研究；不能同时加 `--file` |
| `research watch ID [--timeout SECONDS]` / `research wait ID [--timeout SECONDS]` | 状态流或等待结束；ID 为 conversationId |
| `research stop ID` | 请求停止，之后读回状态 |
| `research resume ID --submission-id PREVIOUS_TERMINAL_SUBMISSION [--instruction TEXT]` | 使用 diagnostics 确认的原终止 submission |
| `research delete ID` | 隐藏可删除研究，保留原始证据 |
| `portfolio list` / `portfolio get ID` / `portfolio delete ID` | 组合列表、结果、隐藏 |
| `portfolio watch ID [--timeout SECONDS]` / `portfolio wait ID [--timeout SECONDS]` | 等待同一组合结束 |


旧 `portfolio replay-ticks` / `tick-replay` 服务路由已移除；不要用历史 TickEconomicReplayStart 类型构造新请求。原生回放通过 `research` 执行。

## PM、Campaign 与 Strategy

| 命令 | 请求或 ID 类型 |
| --- | --- |
| `pm capabilities` / `pm list` / `pm get ID` | 能力、mandate 列表及完整视图 |
| `pm decision ID` / `pm history ID` / `pm events ID` | mandate ID；events 是一次 JSON 查询 |
| `pm recompute ID --file COMMAND.json` | `PmCommand` |
| `pm refresh-data ID --file COMMAND.json` | `PmCommand`，捕获已存在数据的新修订 |
| `pm targets ID --file COMMAND.json` | `PmCommand` 加 `decisionId`，显式请求目标交付 |
| `pm retire ID --file COMMAND.json` | `PmCommand`，可选 `strategyVersionId`；退出策略运行路径 |
| `pm delete ID` | 隐藏可删除 mandate |
| `pm cycle ID` | cycle ID，不是 mandate ID |
| `pm capacity ID` | capacity estimate ID，不是 mandate/portfolio ID |
| `campaign get ID` / `campaign trials ID` / `campaign compare ID` | campaign ID |
| `strategy versions ID` / `strategy assessments ID` | 可查询策略版本或定义 ID |
| `strategy pause ID --file COMMAND.json` / `strategy resume ID --file COMMAND.json` | 使用当前 strategy version ID；修订号来自所属 mandate |

## 账户与资金

| 命令 | 请求或含义 |
| --- | --- |
| `observation start --file REQUEST.json` | `ObservationStart`，绑定已存在连接 |
| `observation list` / `observation get ID` | 全部观测或指定观测 |
| `observation refresh ID` / `observation stop ID` | 刷新账户事实或停止观测 |
| `capital status OBSERVATION_ID` | 刷新并读取资金退出状态与命令状态 |
| `capital preview OBSERVATION_ID --file REQUEST.json` | 退出计划预览；仍需要原 `requestId` |
| `capital command OBSERVATION_ID --file REQUEST.json` | 提交资金退出动作；请求字段见账户参考 |

## 设置、扩展与维护

| 命令 | 请求或检查 |
| --- | --- |
| `settings get` | 读取默认模型、thinkingLevel、serviceTier、tickBookCores |
| `settings set --file SETTINGS.json` | PATCH 所需设置，完成后用 `settings get` 读回 |
| `settings set --tick-book-cores COUNT` | 正安全整数；不能同时带 `--file` |
| `extensions list` | 查看名称、enabled、required、state、error |
| `extensions enable NAME` / `extensions disable NAME` | 使用实际名称，读回 loaded/disabled/pending_restart/error；required 扩展可能拒绝关闭 |
| `maintenance status` | 读取 RUNNING/DRAINING/QUIESCED、epoch 与 blockers |
| `maintenance enter --file REQUEST.json` | `MaintenanceEnter`：`requestId`，可选 `reason` |
| `maintenance resume --file REQUEST.json` | `MaintenanceResume`：新操作的 `requestId`、当前 `epoch` |

维护进入回执不等于已经 `QUIESCED`。读取 `blockingTasks`、`blockingSubmissions`、`blockingExternal` 和 `activeBusinessRequests`；不要为消除阻塞而自行停止工作。恢复必须使用读回的 epoch；重放原维护请求保留原 epoch 和 ID。维护命令本身不更新软件，也不改变研究结论。

## 原生扫描与账户策略

扫描命令：`parameter-scan options/preview/create/list/get/stop/trials/surface/watch/wait`。请求与证据限制见 [原生研究](research-and-portfolios.md)。

账户命令：`account-strategy status OBSERVATION_ID`、`receipt OBSERVATION_ID REQUEST_ID`、`start OBSERVATION_ID --request-id ID --account-source-id SOURCE --expected-revision REV`、`stop OBSERVATION_ID --request-id ID`。START 的来源及修订不能自动更新。详见 [账户与资金](accounts.md)。

`observation size ID --file SCENARIO.json` 与 `preview-order ID --file ORDER.json` 是只读能力，不提交订单。`capital context ID`、`capital get OBSERVATION_ID EXIT_ID` 读取现有退出信息；`observation cancel-protection ID` 是显式账户控制，不是回测功能。
