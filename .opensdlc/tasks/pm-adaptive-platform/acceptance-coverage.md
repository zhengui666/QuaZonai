# PM 实施与可执行验收映射

本表对应 [46 个业务反例](../quant-pipeline-integrity/design/acceptance.md)，以它们要求的业务结论为验收单位。开发基线为 `ea1a900171c93e37cf7de5576d9c6acc09553fb4`；工作区内实现、局部回归、最终提交 CI、合并及部署分别记录。旧分支测试、方案审阅通过和合成数据结果都不升级为本轮业务验收通过。

## D1–D5 的可交付边界

| 分期 | 实现及验证入口 | 必须证明的结果 | 当前证据边界 |
| --- | --- | --- | --- |
| D1 自动比较及结项 | `pm-metrics`、`pm-comparison`、`pm-ledger`、`decision.ts`；T1/T2 | 同 scope、精确 metric identity、完整净收益路径、失败预算保留、原业务偏好决定选择或保留前沿 | 纯比较与 HTTP/Durable 规则已检验；T5 真实原生多日学习→账户→决策正链已通过。完整真实源/尾部统计资格仍须专项证据 |
| D2 因子成员及权重学习 | `ensemble_fit`、原始 factor reports、折内校准和 OOF、`study-portfolio`；T3/T5 | 训练/验证时钟、原始 panel 对齐、零权重合法、重复暴露不加倍、真实账户与基准比较 | 原生受控 Parquet/Wasm 科学测试；不等同未来净收益改善 |
| D3 资金与资产共同求解 | `capital-curve --plan-only` → 每个金额重新 `study-portfolio`；T4/T5 | 原生最小单位及当前资金生成候选；每点重求资产配置并运行共享账户；失败保留，不插值可行性 | `capital-curve` 固定目标回放只是子能力；计划模式没有 TESTED_POINT。经济容量缺冲击/盘口/退出识别时必须不可用 |
| D4 版本化持续更新 | 官方 Durable 的 PM Doc/Task，事件、basis、账户、策略和目标；T2/T5/T6 | 重复事件不重复消耗；旧 basis 不覆盖新决定；暂停、恢复和外部回执分别处理 | 已验证真实文件来源修订、原预算不重置、原决策/回执恢复；双修订原生重拟合验收仍进行中。真实账户与源质量另需资料 |
| D5 完整自适应方法资格 | 原始搜索家族及协议、前推过程、真实源 shadow；T5/T7 | 整个更新过程而非最后权重通过合法独立评估；测量资源与时效，回执不冒充收益 | 本环境模型 catalog 有精确 `gpt-6-luna/max`，所有对应 provider 未认证，真实 generation 未执行。没有生产部署或真实收益资格结论 |

## 可执行证据入口

| 代号 | 命令/测试 | 证据性质 |
| --- | --- | --- |
| T1 | `node --experimental-strip-types --test apps/service/test/pm-contract-acceptance.test.ts` | 独立合成 DTO/决策反例；不是原生指标或市场证据 |
| T2 | `node --experimental-strip-types --test apps/service/test/pm-business-acceptance.test.ts apps/service/test/pm-operations.test.ts`；service/research 的 recovery suites | 真实 HTTP、官方 SQLite/Durable；运维测试的资格/账户事实、模型和 controlled-job 是明确协议 fixture，不冒充投资资格 |
| T3 | `cargo test --locked -p job --features job/native-paper-test,job/native-sandbox-test,job/catalog-prepare,job/polymarket-history,job/hyperliquid-offline --test ensemble_fit --test allocation --test spot_cash_runtime -- --test-threads=1` | 真实 Nautilus/Clarabel、合成 Parquet/Wasm；非模型质量或实盘投资证据 |
| T4 | 同 T3 features，`--test capital_curve -- --test-threads=1` | 真正逐点重跑、PIT 最小单位、失败预算、精确资金；plan-only 必须没有 replay 和 tested point |
| T5 | `make check-core QZ_JOB_BINARY=/absolute/matching/job` 及 PM 原生 service integration | 必须使用与提交一致的 binary、真实科学 fixture；单独提供假 native JSON 不满足此门槛 |
| T6 | `node --test deploy/docker/pm-maintenance.test.mjs deploy/docker/manage_shell.test.mjs`；`node --experimental-strip-types --test apps/service/test/pm-maintenance.test.ts` | 真实官方 SQLite、HTTP QUIESCED/原等待任务恢复、tar 与事实原值；Docker/PG 进程边界是明确 fixture，不能宣称已部署镜像 |
| T7 | 配置真实受权 provider/data/account 后执行完整前推和 shadow 研究 | 本环境尚未执行；禁止用 faux 或替换模型冒充 `gpt-6-luna/max` |

所有 Cargo 命令在本次共享环境使用 `CARGO_TARGET_DIR=/workspace/QuaZonai/target`、`CARGO_PROFILE_DEV_DEBUG=0`、`CARGO_PROFILE_TEST_DEBUG=0`、`CARGO_BUILD_JOBS=2`、`CARGO_INCREMENTAL=0`，并以 `flock /tmp/qz-pm-cargo.lock` 串行。最终自动检查入口为 [Makefile](../../../Makefile)，最终提交状态由 [任务记录](task.md) 承载；本表不是整套业务已通过的声明。

## 46 个场景逐项映射

“部分”表示已有可执行子断言，但未覆盖该场景所有业务证据；“待链路验收”表示需要原生/官方持久化端到端核验；“需真实资料”不能靠新增几个 fixture 测试消除。最终交付应将各项新增实际结果补入任务记录，不能把表行数计成通过数。

| 场景 | 关键业务 oracle | 入口与当前边界 |
| --- | --- | --- |
| A01 同 Sharpe、不同亏损路径 | 净收益排序、回撤 2.9701%/1% 和连亏 3/1 分开；没有偏好不造冠军 | T1 已有精确指标身份比较；完整路径生产与 PM 展示待 T5 |
| A02 同比率、不同本金收益 | 同本金下收益金额不同；禁止凭同比率自动加杠杆 | T1/T4 精确本金部分；实际金额指标和选择待 T5 |
| A03 胜率高、尾部大 | 保留收益/尾部权衡，不用胜率替代账户净收益 | T1 前沿/偏好部分；完整尾部统计待 T5 |
| A04 缺失日不是零 | 缺失/未定价与有证据零收益分开 | T3 spot calendar coverage 部分；跨源 coverage 待 T5 |
| A05 入金不是收益 | 外部流入 500 不将 1000→1500 记成 50% 收益 | 需账户现金流适配和 T5；T4 仅资金守恒，不能替代 |
| A06 费用不重复扣 | 原净 PnL 8 保持 8，费用 2 作归因 | T3 native fee/spot 部分；PM 展示与跨币种待 T5 |
| A07 未平仓损失 | 判断来自完整账户含未实现净值，缺估值标未知 | T3 原生账户部分；账户外部事实待 T5/T7 |
| A08 复制因子 | 不能增加经济信念、分配本金或独立样本数 | T1 duplicate 身份 + T3 原生共线/零权重；独立经济等价判定仍有边界 |
| A09 互补因子与成本 | 低成本可改善，高成本可抵消；共同账户重求组合 | T3 折内拟合/真实 study；需 T5 成本对照链路 |
| A10 共享容量不可加总 | 成员共享同一市场资源约束 | T1/T4 不推断容量；真实共享流动性与退出识别需资料 |
| A11 容量改变配置 | 每资本点重求资产权重，而不是单纯缩放 | T5 已真实执行 NAV=3000 下 K=3000/1500/2250/750，每点原生 study 重新求解并保留实际预算；不是固定目标 curve 或未测区间插值 |
| A12 小本金非线性 | 两个成功金额之间允许失败；不可填补未测洞 | T4 已实证 5/500 完成、252.5 因 lot rounding 拒绝；T1 验 TESTED_POINT 不变区间 |
| A13 当前资金/持仓变化 | 资金守恒到 decimal 最后位，现仓进入过渡成本 | T1/T4 精确 reserve/deployable 部分；真实现仓成本待 T5/T7 |
| A14 经济容量未知 | 不用参与率或固定 fill 缩放制造经济容量 | T4 输出 UNAVAILABLE；T5 必须保留同等限制；需真实冲击/退出资料才能晋级 |
| A15 微小更新保持原仓 | HOLD 与重配同 scope 计净成本后选择 | T1 OperatingComparison identity 部分；实际 incumbent holdings 路径待 T5 |
| A16 目标到期重算 | 不延长原目标 TTL 冒充新证据 | 官方原生 expiry 回归 + PM cycle 新 basis；待 T5 |
| A17 因子失效/执行失真 | 归因支持不同调查，不盲目重训因子 | 需真实执行/成熟标签与 T5/T7；基础设施错误不得经济证伪 |
| A18 并行搜索家族 | 跨任务/会话保留尝试及暴露总量 | 官方研究预算账本和 T5 正链实际尝试保留；跨多个独立任务的统计搜索暴露完整性仍需专项验收 |
| A19 更换目标也是搜索 | 方法变体计入家族，不重置样本资格 | 新精确 policy/metric identity + family；待 T2/T5 |
| A20 标签重叠泄露 | purge/embargo 与标签成熟时钟合法 | T3 malformed folds/future labels；完整方法前推需 T5/T7 |
| A21 已见独立样本 | 反复查看不保留独立资格 | T1 独立 partition 不参与选择；T5 正链已核验唯一 FINAL 暴露、OWNER_ONLY 读原证据不改变选择。多次自适应方法资格仍需 T7 |
| A22 连续描述与显著性 | 描述刷新不自动制造显著性 | T1 指标/证据边界部分；长期监测资格需 T7 |
| A23 新权重不继承独立通过 | 新 EnsembleRevision 及真实来源保留 | T3 fit/apply/freeze provenance；PM 发布链路待 T5 |
| A24 历史修订不倒写 | 原 DatasetRevision/decision 仍可重现 | 真实文件修订/旧快照/冲突身份已验证；HTTP 修订使旧 basis 失效且不重置预算已验证；双修订原生重拟合仍待 T5 新用例 |
| A25 耗尽预算仍结项 | 不新增试验，失败保留且解释选择/不选择 | T1 all failed→NEEDS_EVIDENCE；T4 每失败点计费；官方研究任务已验证最后预算后自动结项、超预算拒绝及恢复不重复消费 |
| A26 退役不等于平仓 | 剩余仓位与未决回执仍需对账 | T2 已验证真实退休HTTP、原任务重启后只GET对账，保留现仓及reconciliationRequired；T6 未决retirement阻止维护。实际退出需 T7 |
| A27 数据恢复后继续研究 | 原身份恢复，不重建预算或读未来资料 | 官方 recovery 基础 + PM gap/event；待 T2/T5 |
| A28 因子单位不同 | 原始信号必须合法校准，不直接拼权重 | T3 折内标准化与 calibration；不同策略原型不强行当 alpha |
| A29 重算与信息变化 | 幂等 event/request，同 basis 不重复消耗 | T2 原请求 replay/重启、同decision换requestId不重复目标 POST 及原receipt身份；T5 正链重启保留决策/费用/科学预算，双修订独立更新仍在验收 |
| A30 无解不等权 fallback | 显式不可行/缺证据，保留原求解原因 | T3 allocation + T4 native failed；T5 不得吞错误换默认配置 |
| A31 无盘口做市 | capability 拒绝或降级条件研究 | 真实 HTTP 已验证不支持做市的意图不偷偷变为方向 alpha；保留 capability gap，不声称 bar-only 支持做市 |
| A32 不同步套利 | 双腿事件/可得时间、资金与退出同步 | 需双腿能力与真实源；当前 predictive-alpha 测试不覆盖 |
| A33 因子独立研究 | 因子证据不隐式依赖已存在组合/账户 | 原 factor CLI + T3；PM 事实链路待 T5 |
| A34 原生/服务能力交集 | 原生可做不代表服务已接线、数据可用或授权 | CLI/合同/服务 adapter 集成 T5；不得仅看 feature flag |
| A35 退出/结算机制 | 原始结算/退出规则进入适用域和证据 | 原生 settlement/spot 局部能力；跨市场 PM 晋级需 T5/T7 |
| A36 历史成分 | 不用现成分回填历史宇宙 | 需历史 universe/source 修订；当前合成固定宇宙不能证明 |
| A37 信息价值预算 | 预算分配考虑预期信息增益并保留结项资源 | 原预算 ledger 可测；自动信息价值策略资格待 T5/T7 |
| A38 基础设施失败 | 技术 FAILED 不是经济假设被否证 | T1 all-failed + T4 missing-catalog/lot failures保留；T2 恢复原失败 |
| A39 退役后损益 | 旧决策剩余持仓损益继续归因 | T6 维护保留退役责任；真实后续损益归因需 T7 |
| A40 新证据重开 | 新版本关联原家族/失败历史，不改名字洗白 | PM revision/retirement facts 与 ledger；待 T2/T5 |
| A41 一次授权自主运行 | 授权边界内任务推进，不让 PM 手配统计参数 | T2 HTTP admission/偏好冻结；真实模型自主研究需 T7（当前认证不可用） |
| A42 研究暂停/目标暂停 | 两种控制不混淆，不假定外部订单取消 | 官方控制任务独立拥有暂停/恢复意图，恢复原 Study；T2/T6 未决回执与 retirement 保留，维护不代表远端取消 |
| A43 独立审阅不是投票 | 独立反例和原始证据，而不是 agent 数量计票 | T1 独立 oracle + T3/T4 科学 oracle；最终独立 review 记录 |
| A44 迟到数据 | availableAt 截止，不逆向影响旧决定 | T1 late metric、T4 causal minlot；dataset事件整体待 T5 |
| A45 费用/规则泄露 | 规则修订也受原可得时钟约束 | 原生历史定义/费用与 T4 instrument clock；跨源完整性需 T5/T7 |
| A46 跨原型闭环 | 多原型各自能力、研究、决定、监测、退出可迁移 | 当前预测型实现不能声称全原型；未支持项须明确 capability gap，需分原型实测 |

## 本验证分工已取得的实际结果

- `capital_curve` 包括 plan-only 的 14 个真实 CLI 科学用例通过；含真实 Clarabel 目标、原冻结 exposure_tolerance、margin 空头和极小权重造成最小入场金额超 NAV 的规划反例。零仅为数值锚点，不被回放、不推导可行区间。小资金拒绝保留为正向业务证据，并未修改冻结模型/tolerance 使其通过。新 observed-spot 合同加入后已再次运行官方生成器，匹配精确快照 1/1 通过；新 SCIENCE_FEATURES/hyperliquid-offline 的 job/source-tools/catalog-prepare/browser fixture 已同轮构建并以硬链接复用。
- 新 `pm-maintenance` 七例加既有 `manage_shell` 六例：13/13 通过，真实 SQLite/文件归档恢复；历史 outcome 的 UNCERTAIN 仅由同 decision/command 身份的新终态回执解除，不改写历史事实。Docker/PG 边界为 fixture。
- `pm-operations` 原七例通过，新增维护用例单独通过：实际一次 POST、丢响应后重启只 GET 原回执、同决定不同请求不二次投递、外部身份冲突不污染事实；完整账户来源与当前 assessment 必须匹配。六例隔离研究 monitor；另一个真实 MonitorMandate/HTTP account GET 用例证明缺失/非零原生订单报告不能确认平仓，完整零订单和原余额证据才允许关闭退休责任。另一个真实自动策略→退休→维护用例证明原 AdjustAccount parked 后仍可只读刷新账户，责任解除才 QUIESCED，原 task ID/waiting 状态保留且 POST 为零。账户/资格/原生源报告均明确受控，不据此宣称真实券商账户、科学投资资格或收益。
- service `pm-maintenance` 5/5 通过：真实 HTTP enter→QUIESCED→关闭→实际停止写入后的运维检查→重开→原 epoch/task→resume；未知任务、未决回执及身份冲突保留。该结果由维护实现分工运行，不是镜像发布或生产切换。
- `make check-deploy` 在本轮先前快照通过，227 source/19 Node/2 package/3 release；新增维护反例已另做上述 13 项 focused 验证。当时缺 release fixture 的 1 个安装场景另以真实本地 Node/CLI 归档补跑，2/2 安装测试通过且零跳过。该归档未发布，不能作为最终 immutable release 或生产运行证据。
- 精确 `gpt-6-luna` 的 `max` 存在于官方 pi-ai 1.1.0 静态目录；8 个匹配 provider 的 `checkAuth` 全为 false。`allowNetwork:false`，generation 未执行，fallback 未执行。静态目录能力不等于账户可调用或模型研究质量。
- 尚无本任务确认的生产运行目标。未读取/修改其他会话研究 1208/1347，未读取用户认证文件，未恢复用户数据库，未发布镜像或部署。

最终应以本次提交 CI/独立审阅/merge 及 [运行维护说明](operations.md) 的实际记录补充交付状态；任何本地 smoke 成功都不能升级成生产已部署。
