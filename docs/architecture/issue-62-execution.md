# Issue #62：集成实施记录

规范来源是 Issue #62 正文及规范附录 A、B。本文记录实现证据，不降低其要求。
旧 DESIGN.md 的 #58 Python 实现结构不约束新 Rust 系统；在新系统整体切换前，
旧服务仍是主干运行事实，不能把新增 Rust 文件误报为已经替换旧服务。

## 首个工作包：W0 原生依赖可行性

`apps/qz-job` 是一次启动、完成验证后退出的 Rust/PyO3 原生依赖探针。
它直接调用 skfolio MeanRisk/CLARABEL、Nautilus BacktestEngine 与上游示例策略、
PyArrow IPC，不嵌入 Python 源码、不编写自己的优化器或回测器。
所有输入和输出显著标记 FIXTURE，不产生 Qualification、Release 或可领取交付。

数值参考为两资产对角协方差比例 1:4 的 long-only 最小方差问题，原生求解结果
应为 0.8/0.2，绝对容差 1e-5。参考公式仅属于测试，不能用于生产兜底。
Nautilus 使用上游 test-kit equity 与 EMACrossLongOnly，要求实际处理 64 根
合成 Bar 并产生原生成交报告；这不等于已经完成目标组合适配、共享资金组合验收
或任何真实交易集成。

候选版本：Rust 1.90.0、PyO3 0.25.1、CPython 3.12.12、Nautilus 1.231.0、
skfolio 1.0.3、CVXPY-base 1.7.2、CLARABEL 0.11.1、PyArrow 25.0.0。
只有实际 CI 输出可以证明这个组合可用；版本声明本身不是测试结果。
许可证沿用根目录 LICENSE/NOTICE；调用的上游库仍受各自许可证约束。

## 验证命令

在依赖已按生成锁文件安装的隔离开发环境中执行：

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p qz-job -- verify-native --output /tmp/qz-native-probe-new
```

输出目录必须不存在。探针不会覆盖历史证据；失败不写成功报告。
`native-probe.json` 是兼容性报告，`native-weights.arrow` 是可回读的 Arrow 结果。

## 完整交付门槛（未被本工作包替代）

W0–W8 和 T01–T42 全部仍是本次同一集成 PR 的必须范围。特别是完整 Rust API /
领域持久化 / Worker / CLI / MCP、原生 Codex 真闭环、远端隔离、Ant Design 全量
迁移、多 Alpha 组合、Paper/Live 目标包、Forward/Wake、数据迁移、备份恢复、
真实截图、受保护账号验收目前不能从这个探针推导为完成。

依赖 bootstrap 工作流只用于生成锁文件和收集原生运行证据，不是最终 required
checks。它不使用生产密钥、不自行提交代码、不合并 PR，不取代 locked build。
普通 CI、旧 Python CI 或原生探针通过都不能宣称 rewrite-complete。

完成边界不变：最新 Head 全部适用 CI 通过、相关 PR 的 Codex review 明确无问题、
所有 review threads 已解决，之后才可合并；再核验 main、回填全部证据后关闭 #62。
禁止在 GitHub 要求 Codex 修复、实现或提交。它只参与代码审查。
