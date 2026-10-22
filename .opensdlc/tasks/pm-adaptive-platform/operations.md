# PM 平台运行、恢复与验证交付

本任务沿用 [工程运维规范](../../operations.md)、[Docker 运维入口](../../../deploy/docker/README.md) 和官方 Pi Durable，不新增运行时、队列或替代数据库。本地受控验证、镜像发布、运行目标切换和真实收益验证分别记录。

## 运行目标与依赖

当前实现工作区以远端 dev `ea1a900171c93e37cf7de5576d9c6acc09553fb4` 为基线，Node `22.19.0`、Rust `1.98.1`、官方 Pi `1.1.0`。测试仅使用本任务临时目录及共享科学编译缓存。没有确认用户希望切换的真实安装目标，也不对其他聊天研究 1208/1347 进行探测或改动。

当前模型能力探测仅使用官方 pi-ai 静态目录，关闭网络刷新，不打开用户凭据文件：精确 `gpt-6-luna` 支持 `max`，匹配 provider 全部未配置认证。未发起真实模型生成或用别的模型替代。配置真实 provider、受权数据及账户后，才能执行完整自适应方法和真实源 shadow 验收。模型 catalog、本地科学运行和实际账户可调用是三个独立事实。

## PM 持久化与停止边界

`qz.pm` 是原 Durable SQLite 中的 session document；Mandate、数据修订、业务事件、cycle、决策、容量及证据、预算、目标回执和退役责任随整个原始数据目录一起保留。任务身份和执行状态仍属于官方 Durable。不要复制几个业务 JSON 后重新建库，也不要把 Doc 快照当任务恢复协议。

部署更新/恢复使用现有 `manage.sh`：停止原 writer，核验已停止；通过官方 Storage API 检查 pending/running/waiting/completing tasks 和 queued/placed submissions；核对旧 application observation 及新 `qz.pm` 外部事实。普通活动任务都阻止维护；唯一例外是下述正式 QUIESCED epoch 中精确登记的原只读等待任务及其官方 gate。PM command/outcome 的未决回执、旧自动 allocation/资本 command 的 `PENDING`/`UNCERTAIN`、退役 `reconciliationRequired=true` 仍阻止维护放行。检查不把外部结果改为成功、不取消目标，也不伪造清仓。

ObservedOutcome 保留观察时刻的回执状态；若其精确 targetCommandId/decisionId 对应的原命令后来已经 ACKNOWLEDGED、REJECTED 或明确 NOT_OFFERED，可据该新事实解除维护阻塞，同时保留历史 outcome 原值。缺少原命令、身份冲突或仍未决不能视作已解决。research pause 不等于维护暂停，不能删除 task 或伪造终态来更新安装。

通过现有已认证 owner HTTP 会话执行以下维护协议；这是操作控制，不是新增模型/统计参数：

1. `POST /api/v3/maintenance/enter`，正文为 `{ "requestId": "ORIGINAL_ENTER_ID", "reason": "planned update" }`；保存返回的原 `receipt.epoch`。丢失响应时重放同一正文，不能更换身份创建另一轮维护。
2. `GET /api/v3/maintenance`，确认该 epoch 达到 `QUIESCED`。`DRAINING` 的 blockingTasks、blockingSubmissions、blockingExternal 或 activeBusinessRequests 非空时，保留原研究/回执继续收束；不能把背景任务统统忽略。未决远端回执和退休责任仍须原事实对账。
3. 使用原安装 `manage.sh` 停 writer 并备份/更新。检查逐项匹配 Application.maintenance 登记的 task ID、kind、epoch、原 resumeCheckpoint，以及官方 waiting/on=[barrierTaskId]/allSettled；只允许 `qz.pm.mandate-monitor`、`qz.account-adjustment` 和匹配的 `qz.maintenance.gate`。未知任务、不同 epoch/检查点、缺失原任务、abort 标记及待处理 submission 均拒绝。任务身份、研究预算、等待检查点和外部责任完整保留。
4. 新进程打开原存储后仍处在同一维护 epoch。核对原身份及版本，再 `POST /api/v3/maintenance/resume`，正文为 `{ "requestId": "ORIGINAL_RESUME_ID", "epoch": "RETURNED_EPOCH" }`。官方 gate 结束后，原等待任务从保留检查点恢复；不是重建任务或重发目标。恢复到不同备份时先读取该备份的实际 epoch，不复用另一安装/恢复点的 epoch。

已配置原服务连接的 CLI 提供同一协议：`quazonai maintenance enter --file ORIGINAL_ENTER_REQUEST.json`、`quazonai maintenance status`、`quazonai maintenance resume --file ORIGINAL_RESUME_REQUEST.json`。文件保留上文原请求正文；CLI 不替换 epoch 或自动新建请求身份。

## 可复现的 stopped-writer 备份恢复

在确实拥有且准备维护的原安装目录运行仓库发布包的管理入口：

```sh
bash manage.sh status --directory /absolute/original-installation
bash manage.sh backup --directory /absolute/original-installation
```

`backup` 会停止应用并保留停止状态。备份包含整个 `data`、原配置/模块/凭据/对象/SQLite 及 sidecars、原安装描述、存在时的 compose override，另保留原 PostgreSQL dump。保护整个恢复点及原 release bundle；不把 secret 打印到日志。不要在仍有任何 writer 时手工复制 SQLite/WAL，也不要把缺少外部 database 路径的归档当完整备份。

恢复前确认目标是原本要恢复的安装及恢复点；真实数据恢复必须有明确授权。本任务只对临时 fixture 执行恢复：

```sh
bash manage.sh restore /absolute/original-installation/backups/RECOVERY_POINT --directory /absolute/original-installation
```

现有恢复流程先保留当前状态的 stopped backup，再恢复原 PG/data/配置并激活原 release。失败后保留 `pending.json`、`data.restore` 和 pre-restore backup；遵循原阶段恢复，不删除标记强行重试。只有本地 exit 并不能证明外部操作已取消。恢复后核对原 mandate/decision/event/command 身份、decimal 资金和字节级证据、官方 task/submission 身份以及下游原回执。

## 本次新增可执行 smoke

```sh
node --test deploy/docker/pm-maintenance.test.mjs deploy/docker/manage_shell.test.mjs
```

新 PM 测试使用真实官方 SQLite：在无活动本地任务的情况下，未决目标/回执/退役仍阻止维护；精确关联的后续原命令回执可解决历史 outcome 的不确定性，检查前后文档原值不变。恢复测试实际执行 `manage.sh` 归档和数据替换，重新打开原 SQLite，核对原 document ID、请求 replay 事实、精确资金 `3000.000000000000000001`、原 revision 和证据文件原字节。Docker/PG 进程及激活边界明确替身，故结果不是完整镜像部署证明。新增 7 例及旧 6 例已在本地通过。本轮先前快照的 `make check-deploy` 通过：source 227 项、Node 19 项（实际 release 安装当时因缺 fixture 跳过 1 项）、package 2 项、release 3 项；新增历史回执、精确 QUIESCED、101 任务跨分页、排队提交及自动分配/资本回执反例已另做上述 13 项 focused 验证。

```sh
node --experimental-strip-types --test apps/service/test/pm-operations.test.ts
```

另 7 项使用真实 HTTP 下游和官方 Durable/SQLite 验证目标操作：一次 POST 的响应丢失后，重启继续 GET 原幂等身份的回执；同一决定的新 requestId 被拒绝；已绑定 externalId 的 PENDING 回执不会被冲突身份的 ACK 覆盖；退休停止新决策但继续原回执对账，保留已有持仓、精确资金和退休责任。ACK 不能成为成交、平仓或已实现收益。还覆盖完整账户来源和当前 assessment 不匹配时拒绝目标。六例的研究 monitor 仅在 fixture 内通过官方 abort 隔离；第七例保留真实 MonitorMandate，实际从 HTTP 读取新账户序列，只有完整原生余额及显式 count=0 的同基准订单报告才确认平仓，缺失/非零订单报告仍保留退休责任。账户、资格及原生报告明确受控；这不是完整科学资格或真实外部账户的正向证据。

`apps/service/test/pm-maintenance.test.ts` 另 5/5 通过真实 HTTP enter→QUIESCED→关闭服务→实际 `maintenance.mjs` 检查→重开原存储→原 epoch/task resume，验证维护协议与停止写入检查兼容。未知任务、未决外部回执和身份冲突均阻止错误放行；已过 HTTP guard 的在途业务请求仍计入 DRAINING，正式业务事务在维护开始后拒绝新建研究。另有真实 automatic enabled→退休→维护用例通过：原 AdjustAccount 在官方 gate 等待，PM 退休 monitor 继续只 GET 新账户证据，明确余额/挂单归零后才 QUIESCED；政策、原 task ID/waiting 状态保留，全程不发送 target。

随后已用官方 Node 22.19.0 和仓库 `pack-cli` 生成约 42 MiB 的本地 Linux CLI 归档，运行 `smoke-cli` 核验 `pm`、`campaign`、`strategy`、`maintenance` 及原命令和 schemas。提供 `QZ_TEST_CLI_ARCHIVE`/`QZ_TEST_CLI_VERSION` 后，安装/重复安装验证 2/2 通过、零跳过：实际下载边界为本地 fixture，归档解包、无 host Node/npm 的内置 Node 执行、带空格 home 路径、原 client config 保留均真实执行。归档仅为未提交工作区的 `v0.0.0-pm-qa.2` 验证产物，版本 manifest 的 revision 为开发基线；没有发布为不可变正式 release。最终提交仍需对应 CI/package/image smoke，不能把这份归档用于宣称生产部署。

`make check-science` 将专用原生多日测试数据导出至 `QZ_PM_NATIVE_FIXTURE_DIR`（默认位于 PI_SCRATCH_DIR/RUNNER_TEMP/TMPDIR 下）。`make check-core` 要求该目录存在原 `source.json` 与独立生成的下一修订 `source-next.json`，同时传递 `QZ_PM_NATIVE_FIXTURE=.../source.json` 文件路径与 `QZ_PM_NATIVE_FIXTURE_DIR` 目录路径，不允许因缺少 fixture 跳过科学核心验收。实际 catalog 路径以导出文档的 `source.catalog` 为准；不能把多日报告与旧短 catalog 混用。现仓原生验收另要求 `QZ_PM_OBSERVED_FIXTURE_DIR/source.json` 以及该目录下 `science/source.json`，分别绑定短现仓适配反例与完整多日科学路径；缺任何原生导出不得靠跳过获得 core 通过。该新增路径当前仍在原生导出与适配验收中，完成状态以任务实测记录为准。

## 原生资金曲线与资源记录

`job capital-curve --catalog PATH [--dataset-revision-id UUID]` 从原生 catalog/PIT 首次非零入场的交易单位、绝对敞口与当前可用资金自动规划，逐点更换原 simulation 的 starting capital 并实际运行；每点保留完整原生结果或失败原因、耗时及资源计数。原生支持的 margin 空头保持原目标与现金权重语义。它固定原目标路径，不能替代每资本点重新求解资产权重的 `study-portfolio`。

`--plan-only` 仅返回自动金额和原始交易单位来源，`termination=PLANNED_ONLY`、`points=[]`、attempted/completed/failed 都为零；它不是测得的容量曲线。PM 编排再为各计划点建立真实 study/config/calibration 证据并重求解。固定目标回放已验证 5/500 可完成而 252.5 拒绝的非单调反例；不可将最大成功金额解释为所有更小金额都可行。

自动计划始终先纳入当前可用资金。若原目标的最小入场候选不可识别，或因极小正权重而超过当前资金，则保留原因与原来源，使用零作为数值规划锚点，在当前资金额度内按最大间隔继续生成正金额；零不会被回放，不产生 TESTED_POINT，也不成为可行下界。已识别且在资金内的最小入场候选优先保留，但同样不保证手续费、后续漂移或重优化可行。`generation.lower_planning_anchor` 明确记录该区别。当前 14 个资金曲线原生测试已通过，包括真实 Clarabel 目标之和在原冻结 exposure_tolerance 内的保真接入，不增加 epsilon、不重归一化。

预算 maximum_simulations 为最多准入的实际回放数量，失败也计数。maximum_elapsed_ms 使用单调时钟控制新一轮准入，不中止已经运行的原生调用；官方 Task/process owner 保留取消权，没有隐式注入 timeout 或未声明的 fuel/memory ceiling。失败调用的部分 event/CPU/内存/fuel 没有量测，不能把成功结果 event 之和说成所有消耗。经济容量因缺少可识别规模冲击、spread 和退出模型明确 unavailable。

## 交付及后续测量

使用一致 SCIENCE_FEATURES 构建并复用同一 binary，运行 [覆盖映射](acceptance-coverage.md) 的适用验收。最终 PR exact head 需要 CI 和 Codex review；合并与发布/部署分别由实际远端记录确认。时效/SLO 需在真实工作量、源更新、模型调用及原生求解下测量，不用 fixture 的毫秒耗时编造生产承诺。
