# Issue #62：W0 原生复用验证记录

状态：**实施中；不是重写完成证明，不允许据此合并或关闭 #62。**

本记录落实 Issue #62 正文 §3、§15 和附录 B T01/T16/T22 的先行验证。
完整交付合同仍是 Issue 正文与规范附录 A/B；本文件不缩减合同、不替代 DESIGN.md。
现有服务和数据尚未迁移或删除。当前入口是独立的验证程序，不是可部署的产品后端。

## 第一性原理与复用登记

| 边界 | 直接复用 | 本仓库新增的最小工作 | 目前未证明的内容 |
|---|---|---|---|
| Rust → 科学库 | PyO3 0.29.2 的嵌入 CPython API | 短进程导入、参数映射、结果校验与退出码 | 完整 JobSpec/权限/预算/结果采纳 |
| 风险和求解 | skfolio 1.0.4 EmpiricalPrior/EmpiricalCovariance/MeanRisk → CVXPY/CLARABEL | 两资产解析解的测试断言；不实现协方差或求解器 | 多 Alpha 校准/混合、现金/组/容量及不可行场景 |
| 数据和模拟 | NautilusTrader 1.231.0 ParquetDataCatalog/BacktestEngine/官方 EMACross 示例 | 合成 Bar 构造、原生目录往返、原生结果字段检查 | target-only 组合适配、统一资金 T20、各市场结算 |
| 表格产物 | PyArrow 25.0.1 IPC | 原生结果到 Arrow、含元数据的往返检查 | 生产 qz.alpha_signal/qz.portfolio_targets schema |
| 事务消息 | PostgreSQL + PGMQ 1.10.0 原生函数 | 两个独立连接验证回滚、注入故障、提交和归档 | Rust run/attempt/owner_epoch/幂等/预算/SSE |
| 进程资源 | Docker 原生非 root/只读/无网络/cgroup/capabilities | 调用标准参数并验证退出状态 | 恶意代码、随机名 secret、sealed 等 T34/T35 全矩阵 |

没有新增 Python HTTP 服务、Python 业务模块、通用 Agent DAG、队列内核、优化器、
订单模拟器或应用级 hash gate。EMACross 仅作为上游的 ABI/backtest fixture 使用；
它不是新产品的 Alpha 或生产策略，也不能用来宣称完成组合验证。

## 输入、输出及失败语义

`qz-job verify-native <new-output-directory>` 只接受全新目录，现有目录直接失败，
不覆盖既有证据。全部输入固定为 SYNTHETIC；manifest 和 Arrow metadata 都明确
`deliverable=false`，没有通往 Approval/Release/Claim 的入口。

数值 fixture 是两列均值为零、协方差为零、方差比例为 1:4 的四条观测。
在权重非负、总和为 1 下，最小方差权重解析值为 `[0.8, 0.2]`。
使用原生 CLARABEL 严格容差求解，再独立比较绝对误差 `<= 1e-6`。
缺值、NaN、Infinity、错误维度或单资产 100% 均不能通过这个 oracle。
这不是第二份生产优化器，也不将该验证解释成 alpha 具有预测收益。

`evidence.json` 包含 schema_version、测试类型、origin/deliverable、实际 Python/
原生库版本、native solver status、实际权重、容差、Arrow 往返结果和 Nautilus
原生 iterations/orders/events。只有全部步骤成功后才创建 manifest；Nautilus
engine 在正常和异常路径都 dispose，最后通过容器退出释放嵌入解释器和本地线程。
该测试不导出收益统计，避免拿缺样本的 Sharpe 或非有限数值充当证据。

PGMQ fixture 仅允许数据库名 `qz_native_probe`，避免误运行到产品数据库。
第一连接验证显式 ROLLBACK、send 后异常的子事务回滚，再提交合法 run/event/message；
第二连接必须只看见已提交记录，读取唯一消息并用原生 archive 归档。

## 当前构建状态与可重复性边界

初次 W0 必须实际解析依赖，不能凭空编造 Cargo.lock 或 scientific transitive lock。
当前 Docker 构建会输出这两个**候选 lockfile**和 rustfmt 格式化后的源文件到 CI artifact。
下一提交必须审查并纳入版本控制，之后改为 locked/frozen 构建与格式 diff 检查。
初次解析成功不等于附录 B 的 `rust` / `supply-chain` / `rewrite-complete` 通过。
镜像先固定 release tag；CI 记录原生 RepoDigests，验收前将镜像引用固定到确认过的 digest。
不得把不存在的 digest、未运行的检查或 skipped 记为 PASS。

开发验证：

```sh
docker build --target runtime -f runtimes/science/Dockerfile -t qz-w0-science .
mkdir -p native-evidence
docker run --rm --user "$(id -u):$(id -g)" --network none --read-only \
  --cap-drop ALL --security-opt no-new-privileges --pids-limit 128 --cpus 2 --memory 4g \
  --tmpfs /tmp:rw,nosuid,nodev,noexec,size=64m,mode=1777 \
  --mount "type=bind,src=$PWD/native-evidence,dst=/output" \
  qz-w0-science verify-native /output/probe
```

CI 使用同一入口，并保留数值 JSON、Arrow、目录、进程退出状态、锁文件和原生镜像身份。
不访问真实 Codex profile、模型密钥、broker credentials 或生产数据。

## 不可越过的后续交付边界

W0 的真实 Codex 协议、Qlib 兼容、原生 target adapter 以及 W1–W8/T01–T42 的其余内容
仍须在同一集成 PR 完成。不能将该先行提交独立合并为“完成 #62”。
最终条件不变：完整实现 PR；最新 Head 的全部适用 CI 通过；所有 Review Thread 解决；
相关 PR 的 `@codex review` 明确无问题；满足这些条件后才能 merge；main 复核和证据回填
后才关闭 Issue。GitHub 上 Codex 仅用于 review，禁止要求其实现、修复或提交。

## 上游核对依据

- https://github.com/PyO3/pyo3/releases/tag/v0.29.2
- https://pyo3.rs/v0.29.2/python-from-rust.html
- https://github.com/skfolio/skfolio/releases/tag/v1.0.4
- https://skfolio.org/generated/skfolio.optimization.MeanRisk.html
- https://github.com/nautechsystems/nautilus_trader/tree/v1.231.0
- https://github.com/pgmq/pgmq/tree/v1.10.0
- https://github.com/DietrichGebert/ponytail/blob/main/skills/ponytail/SKILL.md
