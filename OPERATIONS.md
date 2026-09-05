# 运行与部署

本分支已实现 Rust 原生组件、逐轮 PostgreSQL Store 和可运行的浏览器认证 API，**尚非完整研究与交付产品**。旧实现已删除，无兼容服务；完整目标和完成条件在 DESIGN。

## 首次启动认证服务

依赖固定 Rust 工具链及 PostgreSQL18 + PGMQ1.10.0，使用独立的新数据库。由原生 PostgreSQL 管理工具创建不带超级用户、创建数据库、创建角色权限的应用登录角色，密码通过交互或受保护配置输入；迁移身份与应用身份分开。

CLI.md 中 `init-state → migrate → bootstrap → serve` 是实际可执行入口。`migrate --application-role NAME` 通过 SQLx 和 tower-sessions 原生迁移创建域表及会话存储，授权应用 DML；`serve` 不执行迁移，并拒绝高权限/owner 数据库连接。生产入口使用同源 HTTPS，监听内部地址并由受信任反向代理终止 TLS、保留 Host；不要将明文内部端口直接暴露公网。

`bootstrap` 只在本机显示一次 `capability_id/capability/expires_at`。浏览器使用该凭据请求 `POST /api/v2/bootstrap/start`，获得只展示一次的原生 `otpauth://` URI；扫码后提交 `/bootstrap/confirm` 的六位动态码。初始化确认与首个登录权限在同一事务提交，完成后所有 bootstrap capability 失效。

正常登录仅提交 TOTP，勾选信任设备时同时提供标签。普通会话12小时，信任设备30天；到期不延长。会话 cookie 由 tower-sessions 原生私有 cookie 管理，HTTP-only、SameSite Strict、根路径、生产 Secure。API 无需/不接受浏览器提交用户名、密码或 cookie 内的自报权限。

## 撤销、重放与故障

每次请求通过 PostgreSQL 的登录权限、设备状态和认证 epoch 复核，不只相信 cookie。注销先提交数据库撤销再删除原生 Session；并发请求保存旧 Session 也不能恢复登录。删除信任设备需最近300秒内 TOTP 验证。动态码按实际匹配的时间步一次性消费，±1步容差不允许重放；全局每操作60秒最多5次验证，多个 API 实例共享数据库限流。

业务、认证响应均 `Cache-Control: no-store`；浏览器写入必须携带与 PUBLIC_URL 完全匹配的 Origin。数据库、Secret Store 或 Session Store 不可用时拒绝操作，不能退回匿名或内存认证。失败响应只包含安全错误和请求编号，不含路径、密钥或 SQL 详情。

## 数据和密钥

私有状态目录包含 master.key、session-key.ref、secrets。master key 为0600的32字节原生随机密钥；每个 secret 使用 RustCrypto XChaCha20Poly1305、独立随机 nonce，并绑定 UUID 和用途。加密对象先同步、只读发布，再写数据库引用。不要把 master key 放进普通数据库备份、源码、Agent workspace 或 job 容器。密钥丢失无法靠数据库恢复，需要独立安全备份。

源码删除不授权删除运行中的旧库、用户 artifacts、备份或 Codex profile。不得将新 schema 直接应用到旧库；实际产品切换仍须完成只读导入、备份恢复和回滚演练。当前没有声称达到 RPO/RTO。

## 尚待完成的产品部署验收

研究/组合/交付 UI、Worker/MCP/Codex 真闭环、受信任 runtime 与 job 隔离、多 Alpha/共享资金、Paper/Live/Forward/Wake，以及完整恢复/迁移仍未完成。普通 PR CI 不携带生产秘密，真实受保护验收只运行经过审查的固定 Head。QZ 不持有 Broker 凭据或真实执行控制权。
