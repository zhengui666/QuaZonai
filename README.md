# QuaZonai

证据优先的自托管量化研究工作台：目标是将想法变成可追溯研究、合格Alpha和目标组合包，不是Broker、交易执行控制面或收益保证。

> **正在重写，尚不可作为完整产品部署。** 本分支已删除旧服务、旧前端和兼容层；不要沿用旧Docker/uv启动命令。当前可运行内容包括真实 Axum 认证/Project/机器凭据与不可变研究准备 API、本机初始化命令、Rust 合同/领域测试、PostgreSQL 逐轮 Store、原生科学计算和 Codex 协议探针。完整 Web 产品、研究/交付、迁移和恢复尚未通过验收，PR #63仍须保持Draft。

## 已有实现与边界

| 内容 | 当前事实 |
|---|---|
| 原生回测 | Nautilus Rust 0.63.0直接调用BacktestEngine/EmaCross，不再使用Python；固定745个synthetic quote，用原生事件/订单计数验证 |
| 原生求解 | Clarabel Rust 0.11.1，最小方差手算参考0.8/0.2及不可行约束测试；不是完整生产组合流程 |
| Arrow | Rust IPC RecordBatch写入/回读，明确FIXTURE不可交付 |
| 领域基础 | 精确UUIDv7/bigint/Decimal、预算、租约/终态、Codex覆盖及required指标判定；不是完整数据库权限证明 |
| 认证 API | Axum + PostgreSQL 原生会话、一次性本机初始化、六位 TOTP 登录、防重放、持久注销/设备撤销；普通服务使用非 owner 数据库角色 |
| Project 与机器身份 | 真正的项目分页/创建/更新、乐观并发、不可变命令回执、机器 token 一次性签发与撤销；机器只读授权项目，人工 CLI 管理操作另需原生 TOTP 单次授权 |
| 研究准备 | 同事务冻结输入集合、不可变评估政策与实验族登记，严格分区/许可/项目关联和分页授权；登记验证意图不代表已运行原生算法、验证 PIT 或得到 PASS |
| PostgreSQL Store | 新库SQLx迁移、逐轮不可变预约/发送/结算、同Mission幂等与预算投影、关系唯一/复合外键；研究/评估权限全链路与 Worker 仍待完整验收 |
| Codex | 锁定官方App Server stdio、全分页模型及Thread启动探针；真实账号/同Thread工具闭环还需验收 |
| 交付与UX | 全量Ant Design、审批/反馈/晋级/唤醒、旧数据导入、恢复与隔离仍在实施，不虚构页面或状态 |

## 开发验证

需要Linux x86_64、原生Rust 1.98.0工具链和C工具链；Nautilus发布族2.0.0rc4仍为RC，不隐瞒预发行风险。此路径不安装Python。

```sh
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace --exclude store --exclude server
cargo build --locked --workspace --all-targets
cargo run --locked -q -p contracts --example generate > /tmp/domain-v1.openapi.json
diff -u contracts/generated/domain-v1.openapi.json /tmp/domain-v1.openapi.json
```

上述命令明确只运行不依赖数据库的单元/原生测试，不等于全量检查。

完整 workspace 检查必须设置指向**可丢弃测试实例**的 `DATABASE_URL`；实例需要 PostgreSQL18、PGMQ1.10.0，以及仅供测试的创建数据库权限。不得使用生产 URL，SQLx 为各测试创建独立新库并应用迁移。缺少连接应失败，而不是跳过：

```sh
# DATABASE_URL must already refer to the disposable test instance described above.
make check
# Run only the actual PostgreSQL transaction/constraint tests:
make check-store
# Test actual Axum routes, cryptography and independent PostgreSQL databases:
make check-http
```

`make check-unit` 是无数据库的明确子集；不能拿其通过替代 Store 测试。
新 CI 的 `store-postgres` job 使用固定原生 PGMQ 镜像，整体基础检查依赖该 job 成功。
这仍不代表 Web/CLI 完整产品或受保护真实账号验收。

执行原生fixture，输出目录必须不存在：

```sh
cargo run --locked -p job -- verify-native --output /tmp/quazonai-native-example
```

`native-probe.json`仅在完整写入/同步后发布；`origin=FIXTURE`、`deliverable=false`。结果不是收益证明，不能生成正式审批/交付。失败、已有目录或不可行约束不会返回隐藏的成功/单资产兜底。

Codex无账号协议探针：

```sh
npm ci --prefix runtimes/codex --ignore-scripts --no-audit --no-fund
export CODEX_NATIVE_BIN="$PWD/runtimes/codex/node_modules/@openai/codex-linux-x64/vendor/x86_64-unknown-linux-musl/bin/codex"
export CODEX_PROBE_DIR=/tmp/quazonai-codex-example
timeout --kill-after=5s 90s cargo run --locked -p job --example codex_contract
```

这使用独立空profile，不读取或修改宿主登录。真实SYSTEM/官方订阅/custom-provider路径不可被此探针替代。

## 文档

- [完整设计和验收合同](DESIGN.md)
- [Rust复用调查与Python例外证据](docs/research/reuse.md)
- [治理](AGENTS.md)、[实际命令](CLI.md)、[运行限制](OPERATIONS.md)
- [实现证据](docs/architecture/issue-62-execution.md)、[兼容矩阵](docs/architecture/compatibility-matrix.md)

目录不用qz-前缀；旧代码只存在Git历史，源码清理不删除用户数据。Rust可用就用Rust，Python须先提交具体能力证据。不自研成熟数值、认证、Agent、队列或容器平台。

完整W0–W8/T01–T42、最新Head全部适用CI、明确无问题Codex review和零未解决线程之前不得合并；旧测试删除或少数检查绿色不代表完整产品通过。


## 已实现的认证 API

`apps/server` 已提供真实 Axum 认证服务与本机 `init-state`、`migrate`、`bootstrap` 命令，
复用 tower-sessions/PostgreSQL、totp-rs、Argon2 和 RustCrypto。初始化需要本机一次性
capability；正常登录只提交六位 TOTP。注销、设备撤销、认证 epoch 与重放检查由数据库
持久化，旧 cookie 不能恢复已撤销权限。运行步骤见 [CLI](CLI.md) 和 [运维](OPERATIONS.md)。

这是可运行的认证 API，不是研究产品已全部完成的声明；前端、研究/组合/交付全链路
仍须按 DESIGN 实现和验收。API 合同由 `server openapi` 从实际路由生成，不手写平行协议。

## License

原创代码保持[AGPL-3.0-only](LICENSE)。第三方代码保留上游许可证，Nautilus示例保留LGPL版权说明，见[NOTICE](NOTICE)和[第三方说明](THIRD_PARTY_NOTICES.md)。

### Run 与事件接口增量

已实现受信任 Store 的事务准入、同 Attempt 接管/取消/终态回执，以及带认证的
Run 查询、取消和持久 SSE HTTP；完整路径和权限见 [CLI](CLI.md)。这不代表
完整 Worker/远端隔离、研究业务、科学资格、前端或交付闭环已完成。
