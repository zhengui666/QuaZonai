# 原生兼容性矩阵

本表记录 W0 兼容性，不授予真实市场、研究方法、发布或完整系统支持资格。
架构以根目录 `DESIGN.md` 为准，历史执行记录见 `issue-62-execution.md`。

| 组件 | 固定版本 | 已验证的接口 | 未由此验证的能力 |
|---|---|---|---|
| Rust / PyO3 | 1.90.0 / 0.25.1 | 编译、Clippy、测试、嵌入 CPython | API/Worker/生产控制面 |
| CPython | 3.12.12 | Linux x86_64，Ubuntu 24.04 CI | Windows、macOS、ARM |
| skfolio / CVXPY-base / CLARABEL | 1.0.3 / 1.7.2 / 0.11.1 | 原生最小方差、optimal 状态、独立数值参考 | 正式多 Alpha/全部约束/DSR/PBO |
| NautilusTrader | 1.231.0 | BacktestEngine、原生 Bar、上游 EMA、成交报告、dispose | 目标组合、多 Alpha 共享资金、真实远端隔离 |
| PyArrow | 25.0.0 | 原生 IPC 写入和相等性回读 | 正式预测 Arrow schema 和发布存储 |
| Codex | 0.144.4 | 原生 schema、stdio、无账号读取、8 页模型、默认和 effort-only Thread | 真实账号、同 Thread 推理/工具、恢复和实际安全隔离 |
| PGMQ | 1.10.0 | PostgreSQL 18 镜像的事务回滚、重复投递、确认回滚 | 生产租约、Attempt/CAS、取消和崩溃恢复 |

## 可复现依赖来源

`Cargo.lock` 由真实 Cargo resolver 生成；CI 不重新生成它，所有 Rust 构建使用
`--locked`。`runtimes/science/requirements.lock` 来自已经通过 `pip --require-hashes`
验证的 Linux x86_64 / CPython 3.12 wheels；每个直接和间接包都有固定版本及原生
wheel integrity hash。它不支持其他平台的替代 wheel，不允许移除 hash 来让安装通过。
`runtimes/codex/package-lock.json` 由 npm 对固定官方包生成，CI 使用 `npm ci`。
PGMQ 使用原生 OCI digest：

```text
ghcr.io/pgmq/pg18-pgmq@sha256:bfb3537068ce453609744518ece92b178ac89dff53747d47ca6fab91c2fc66a6
```

这些是依赖供应链锁定，不是自建 QZ 业务 hash/fingerprint 门禁。版本升级必须在 PR
重新验证相同原生路径，不得把最新主干文档当成旧版本能力。

## 科学库 lock 的维护路径

`requirements.in` 仅记录直接依赖输入，不能直接用于验收安装。依赖维护者在隔离的
指定平台先用固定 resolver 生成包含全部传递依赖与 hashes 的临时完整 lock，再用
`pip download --only-binary=:all: --require-hashes --no-deps` 按该 lock 下载 wheels。
`python tools/lock_science_runtime.py export VERIFIED_WHEEL_DIR runtimes/science/requirements.in NEW_LOCK_PATH` 使用 pip 原生 hash
与标准 wheel 文件名解析生成平台 lock；输出必须是不存在的新路径。核对原始完整
lock、平台 lock、wheel 版本后提交，再让 CI 只消费已提交的 lock，不能在验收任务中
现场重新解依赖。工具不下载未经完整 lock 核验的任意 wheels。

导出器同时记录规范化的直接依赖集合。CI 在安装已锁定的 `packaging` 后执行
`python tools/lock_science_runtime.py check runtimes/science/requirements.in runtimes/science/requirements.lock`，
离线检查新增、删除、版本漂移、缺失项和重复项；不解析或改写依赖。`packaging` 负责
PEP 508/440 解析，平台 lock 仅允许导出器的无条件精确 pin 格式；不支持的 marker、
extras、URL 或递归 requirements 必须明确报错。原生 `pip check` 另外核对已安装的
传递依赖要求。输入一致性不替代 wheel 哈希验证、原生数值验证或完整供应链审计。

历史原生运行通过不代表安全审计完成；许可证、漏洞扫描、完整镜像及 SBOM 均仍须
纳入 #62 最终 supply-chain gate。本表没有把缺失 gate 宣称为已通过。
