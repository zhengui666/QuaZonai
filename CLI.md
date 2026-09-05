# CLI 命令

完整产品合同在 DESIGN。当前已实现原生验证、逐轮 Store 和浏览器认证服务；研究/组合/交付命令仍待实现，不提供绕过 API 的手工 SQL 业务路径。

## 认证服务与本机管理

以下入口复用 Clap；`cargo run --locked -p server -- --help` 展示实际命令。

```sh
# 目录必须不存在；生成私有 master.key、原生 session key 和加密 secrets 目录。
cargo run --locked -p server -- init-state --state-dir ./var

# DATABASE_URL 此时是独立的新库迁移身份。原生 PostgreSQL 管理预先创建
# quazonai_app 登录角色；本命令只授予应用所需 DML，不创建或输出数据库密码。
cargo run --locked -p server -- migrate --application-role quazonai_app

# 将 DATABASE_URL 切换为非 owner、非 superuser 的应用身份。
# 此本机命令显示一次15分钟有效的初始化 capability；没有远程发证接口。
cargo run --locked -p server -- bootstrap

# PUBLIC_URL 必须是实际同源 HTTPS 入口。API 不在启动时执行 DDL。
cargo run --locked -p server -- serve --state-dir ./var \
  --bind 127.0.0.1:8080 --public-url https://research.example
```

`DATABASE_URL` 支持环境变量；不要把真实密码写到命令行、Git 或日志。默认启动拒绝具有 schema CREATE、表 TRUNCATE 或超级用户权限的应用角色。master key 必须独立于数据库和加密对象备份。

本地开发可显式使用 `--development-http --public-url http://127.0.0.1:8080`，同时监听地址必须为 loopback。此选项只调整本地传输和 cookie 的 Secure 属性，不跳过初始化、TOTP、会话撤销、Origin 或数据库角色校验。

## 原生组件与合同验证

```sh
cargo run --locked -p job -- verify-native --output NEW_DIRECTORY
cargo run --locked -q -p contracts --example generate
cargo run --locked -q -p server -- openapi
```

`job` 命令只运行固定 Rust Clarabel/Nautilus/Arrow fixture，输出不可交付；不能生成正式资格或目标包。`contracts` 生成共享 DTO；`server openapi` 生成实际 HTTP 路由合同。原生 Codex 兼容性命令仍为 `cargo run --locked -p job --example codex_contract`，需 `CODEX_NATIVE_BIN` 与不存在的 `CODEX_PROBE_DIR`；不是完整模型工具循环。

## 开发测试

仅对可丢弃的 PostgreSQL18 + PGMQ1.10.0 使用：

```sh
DATABASE_URL=postgres://TEST_USER:TEST_PASSWORD@127.0.0.1:55432/postgres \
  cargo test --locked -p store -p server
```

SQLx 创建独立测试数据库并执行提交的迁移；不要使用生产 DATABASE_URL。HTTP 测试运行真实 Axum、Argon2、TOTP、AEAD、PostgreSQL Session Store，并另测非 owner 角色与 loopback TCP。它们不是完整研究/组合/交付的验收结果。
