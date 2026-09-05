# Issue #62：集成实施证据

`DESIGN.md` 是完整产品与架构事实源。本文只记录实际实施和验证，不建立第二套架构，
不降低 Issue #62 正文及规范附录 A、B 的任何要求。当前 PR #63 仍为 Draft；旧服务
尚未被替换。W0 的兼容性探针成功不等于新产品可用，也不等于 W0 全部完成。

## 已实施的模块

| 模块 | 实际改动 | 明确边界 |
|---|---|---|
| `Cargo.toml`、`Cargo.lock`、`rust-toolchain.toml` | Rust 1.90.0 / PyO3 0.25.1 锁定 workspace | 目前只有 qz-job；不是完整控制面 |
| `apps/qz-job/src/optimization.rs` | 原生 skfolio MeanRisk / CLARABEL，独立 0.8/0.2 数值参考 | 不实现生产组合优化或 fallback |
| `apps/qz-job/src/backtest.rs` | 原生 Nautilus、64 根 Bar、原生 EMA 策略与成交报告，dispose | 不等于目标组合或共享资金多 Alpha 模拟 |
| `apps/qz-job/src/arrow.rs` | 原生 Arrow IPC 写入与回读比较 | 全部产物 FIXTURE、非交付 |
| `apps/qz-job/src/report.rs` | 临时文件完整写入/同步后原子且不覆盖地发布报告 | 不宣称生产 Artifact Store 或目录崩溃一致性 |
| `apps/qz-job/examples/codex_contract.rs` | 原生进程握手、无账号读取、完整分页、默认/effort-only Thread | 没有真实账号、推理或同 Thread 工具闭环 |
| `tests/native/pgmq_contract.sql` | 真 PostgreSQL / PGMQ 的事务、重复投递、结果与确认回滚 | 临时事实表不是生产 Run/Attempt |
| `runtimes/*` | 已验证科学库平台 hash lock、npm lock、原生版本 | 仅声明验证过的平台；不使用 latest |
| `.github/workflows/rewrite-bootstrap.yml` | committed-source fmt check、locked 构建、真原生验证、不可变 PGMQ 镜像 | W0 兼容性工作流不是最终 rewrite-complete |

## 已取得的历史证据

2026-09-05，Head `c28b49385a2c522bb292e1f594545178648df9ab` 的
[Actions run 33946019806](https://github.com/zhengui666/QuaZonai/actions/runs/33946019806)
中 `native-bootstrap` 和 `pgmq-bootstrap` 全部步骤成功。

- skfolio 原生权重为 `0.8000000000302019 / 0.19999999996979811`，绝对容差仍为
  `1e-5`；默认求解精度曾不满足该容差，修复使用原生 objective scaling 和 solver
  tolerances，没有放宽数值断言。
- Nautilus 真实处理 64 根 Bar，并产生 2 条原生成交记录；Arrow 回读一致。
- Codex 0.144.4 原生 model/list 完整返回 8 页、8 个模型；无账号读取、两个独立
  Thread 的默认与 effort-only 配置均通过。报告明确写入
  `real_account_acceptance=false`、`same_thread_inference_tool_loop=false`。
- PGMQ 1.10.0 事务回滚、重复读取和确认回滚通过；镜像原生 digest 现已固定。

上述是对应 Head 的历史运行事实，不是后续 Head 自动继承的通过结论。PR workflow
检验的是 GitHub 的临时 merge ref；产物中的 `tested-commit.txt` 是实际测试提交，
不是已经合并到 main 的证据。最终状态只以最新 Head 的新 CI/Review 为准。

## 本地与 CI 复现

平台、依赖和限制见 `compatibility-matrix.md`。在 CPython 3.12.12 / Linux x86_64
隔离环境中安装已提交的 wheel lock，再执行：

```sh
python -m pip install --only-binary=:all: --require-hashes -r runtimes/science/requirements.lock
export PYO3_PYTHON="$(command -v python)"
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
python tests/native/test_upstream_compatibility.py
cargo run --locked -p qz-job -- verify-native --output /tmp/qz-native-probe-new
```

输出目录必须不存在；成功报告只能在全部报告写入与文件同步成功后出现。
同步、序列化、已有目标、非法文件名和成功发布均有 Rust 回归测试。探针错误输出
不包含上游原始异常或账号数据；不能从失败目录里的单独 Arrow 文件推断成功。

Codex 合同测试必须使用新 HOME，直接拥有原生子进程；它不读取系统登录态。真实
SYSTEM/CUSTOM、现有账号和同 Thread 工具回读属于尚未完成的受保护验收。

## 尚未完成，不能合并或关闭

W0 的科学容器、远端实际隔离/回收和真实账号完整闸门尚未完成。W1–W8 的正式模型、
迁移、Rust API/Store/Worker/CLI/MCP、真实 Agent 闭环、PIT/封存、至少两个 Alpha 的
原生组合、共享资金模拟、Release/审批/领取、Forward/Wake、Ant Design 全量页面、
移动/PWA、恢复、真实截图和完整 T01–T42 均不能由这些探针替代。

没有创建这些必需能力的实现或 required checks，就不能因为已有工作流绿色而宣称
#62 已完成。真实账号或授权数据验收缺少条件时为 BLOCKED，不是 skipped pass。

结束边界仍是：同一 PR 完成全部范围，最新 Head 全部适用 CI 通过、Codex review
明确无问题、所有 threads 解决；之后才允许合并，再核验 main 并回填完整验收证据
后关闭 #62。GitHub 上 Codex 仅参与 review，禁止要求其修复、实现或提交。
