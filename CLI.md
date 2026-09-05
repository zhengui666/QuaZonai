# CLI 实现状态

完整目标CLI及权限合同只在DESIGN附录B；本文件不把未实现的命令写成可运行。

## 当前可运行

- `cargo run --locked -p job -- verify-native --output NEW_DIRECTORY`：固定Rust Clarabel/Nautilus/Arrow fixture；目录必须不存在；不会读取用户数据或交付目标包。
- `cargo run --locked -q -p contracts --example generate`：原生生成当前DTO OpenAPI，尚无HTTP endpoints。
- `cargo run --locked -p job --example codex_contract`：需CODEX_NATIVE_BIN和不存在的CODEX_PROBE_DIR；仅原生stdio/模型/Thread兼容性，使用新空profile。

README给出完整可执行参数。没有兼容的旧Python CLI；旧命令已撤下。Operator-only命令、正式数据/项目/审批管理及MCP是仍需完成的实现项，不允许绕过API用手工SQL冒充完成。


## 开发验证：逐轮 Store

这不是尚未实现的用户 API/CLI 命令。仅对可丢弃 PostgreSQL18 + PGMQ1.10.0 执行：

```sh
DATABASE_URL=postgres://TEST_USER:TEST_PASSWORD@127.0.0.1:55432/postgres \
  cargo test --locked -p store
```

SQLx 会创建/清理自己的测试数据库；不得给它生产 DATABASE_URL。迁移通过 SQLx 原生 runner 应用到新数据库，不需要部署旧 Python 服务。`apps/server` 用户入口仍未交付，不能把本命令写成产品 Quickstart。
