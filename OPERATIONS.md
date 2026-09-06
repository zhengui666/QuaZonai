# 运行与部署

本分支已实现 Rust 原生组件、逐轮 PostgreSQL Store 和可运行的浏览器认证 API，**尚非完整研究与交付产品**。旧实现已删除，无兼容服务；完整目标和完成条件在 DESIGN。

## 首次启动认证服务

依赖固定 Rust 工具链及 PostgreSQL18 + PGMQ1.10.0，使用独立的新数据库。由原生 PostgreSQL 管理工具创建不带超级用户、创建数据库、创建角色权限的应用登录角色，密码通过交互或受保护配置输入；迁移身份与应用身份分开。

CLI.md 中 `init-state → migrate → bootstrap → serve` 是实际可执行入口。`migrate --application-role NAME` 通过 SQLx 和 tower-sessions 原生迁移创建域表及会话存储，授权应用 DML；`serve` 不执行迁移，并拒绝高权限/owner 数据库连接。升级前暂停 HTTP/CLI/MCP 写命令和 Worker，并等待旧事务结束；只用 `cargo run --locked -p server -- migrate`，不要在活跃库上直接执行 SQLx CLI 或单条迁移 SQL。该命令先用原生迁移锁和应用表写冲突锁保护整个待应用批次，失败全部回滚；锁超时应排查旧事务后重试，不杀事务或放宽锁跳过验证。0006 安全升级会撤销已初始化实例的全部历史浏览器/设备和一次性 Operator 授权，须重新 TOTP 登录；旧审计记录保留。生产入口使用同源 HTTPS，监听内部地址并由受信任反向代理终止 TLS、保留 Host；不要将明文内部端口直接暴露公网。

`bootstrap` 只在本机显示一次 `capability_id/capability/expires_at`。浏览器使用该凭据请求 `POST /api/v2/bootstrap/start`，获得只展示一次的原生 `otpauth://` URI；扫码后提交 `/bootstrap/confirm` 的六位动态码。初始化确认与首个登录权限在同一事务提交，完成后所有 bootstrap capability 失效。

正常登录仅提交 TOTP，勾选信任设备时同时提供标签。普通会话12小时，信任设备30天；到期不延长。会话 cookie 由 tower-sessions 原生私有 cookie 管理，HTTP-only、SameSite Strict、根路径、生产 Secure。API 无需/不接受浏览器提交用户名、密码或 cookie 内的自报权限。

## 撤销、重放与故障

每次请求通过 PostgreSQL 的登录权限、设备状态和认证 epoch 复核，不只相信 cookie。注销先提交数据库撤销再删除原生 Session；并发请求保存旧 Session 也不能恢复登录。删除信任设备需最近300秒内 TOTP 验证。动态码按实际匹配的时间步一次性消费，±1步容差不允许重放；全局每操作60秒最多5次验证，多个 API 实例共享数据库限流。

业务、认证响应均 `Cache-Control: no-store`；浏览器写入必须携带与 PUBLIC_URL 完全匹配的 Origin。数据库、Secret Store 或 Session Store 不可用时拒绝操作，不能退回匿名或内存认证。失败响应只包含安全错误和请求编号，不含路径、密钥或 SQL 详情。

## 机器凭据和项目管理

项目与身份 API 的实际路径和严格 DTO 由 `cargo run --locked -p server -- openapi` 导出。浏览器登录后使用 `POST /api/v2/projects` 创建项目，`PATCH /api/v2/projects/{id}` 必须带当前 `expected_revision`；所有管理写请求必须提供非空且不超过200字节的 `Idempotency-Key`。重复同键/同请求只返回已提交的原始响应，不把后来修改过的对象冒充首次结果；同键不同内容返回409。项目未绑定已冻结 Brief 不能激活，归档后不能原地复活。

Operator 可创建独立 CLI/AUTOMATION/DOWNSTREAM 主体，系统任务的 MISSION 身份不由公共 API 创建。每个凭据只在首次响应中显示完整 `qz2.<public_id>.<opaque>` token；数据库只登记不可逆原生 verifier 的 SecretVault 引用，列表、回执、日志不含秘密。准确重试签发返回同一凭据和 `token:null`，不是再显示秘密；首次响应丢失时撤销该凭据并以新键重新签发。不要在 URL、命令行参数、issue、Agent prompt 或浏览器持久缓存中放 token。

机器请求只能在 `Authorization: Bearer ...` 中提交一次，不能同时附带浏览器 Cookie。机器读写在业务事务内再次检查 scope、精确 project/run/downstream、到期、撤销和主体 epoch。禁用/重新启用主体都推进 epoch；旧凭据不复活。DOCTOR_READ 是独立只读 CLI/AUTOMATION 权限，不能与其他权限混合、不能授给 Downstream/Mission。

人工 CLI 需要管理操作时，通过 `/auth/operator-command-grants` 提交真实 TOTP、封闭 operation 和完整预期请求，取得最长300秒且一次性的 grant；操作时用 `X-Operator-Grant`。创建资源的 UUID 由服务器选定，已存在资源必须指定精确 target。该授权不改变机器身份、不向 Agent 授予 Operator 权限，AUTOMATION/MISSION/DOWNSTREAM 不能领取。撤销、过期、请求替换、目标替换和再次使用不同键都拒绝；已提交的完全相同重试仅能读原回执。读取回执仍要求当前有效的机器凭据和相同认证 epoch，但先于新的 TOTP 校验与 REAUTH 限流，因此旧动态码过期或新验证额度用尽不会把已提交授权误报为失败；原授权到期时间不延长。

机器 capability 的原生 Argon2 校验前，PostgreSQL 原子预约60秒窗口：每凭据最多5个、全局最多32个失败或在途尝试。成功仅归还所属原窗口的占用，失败、取消和计算槽繁忙保留至窗口重置；429响应含 Retry-After。机器计算使用独立2个槽，不占用浏览器 TOTP 的2个槽；多个实例共享数据库窗口。不要以增加实例绕过限流。

## 数据和密钥

私有状态目录包含 master.key、session-key.ref、secrets。master key 为0600的32字节原生随机密钥；每个 secret 使用 RustCrypto XChaCha20Poly1305、独立随机 nonce，并绑定 UUID 和用途。加密对象先同步、只读发布，再写数据库引用。不要把 master key 放进普通数据库备份、源码、Agent workspace 或 job 容器。密钥丢失无法靠数据库恢复，需要独立安全备份。

机器凭据签发先持有数据库命令事务，确认不是重试后才生成 verifier 文件；并发同键请求不会重复生成。文件成功而数据库失败或提交结果未知时，清理重新取得相同数据库权限行锁，主库确认没有任何历史凭据引用才删除该 UUID 的原生认证 MACHINE_VERIFIER 对象。数据库不可判定时保留对象，不冒险删有效凭据。进程崩溃或取消后的孤儿可以通过本机维护命令回收：

```sh
cargo run --locked -p server -- prune-unpublished-verifiers --state-dir ./var
```

此命令仅删除可用当前密钥认证、用途精确为 MACHINE_VERIFIER 且没有历史凭据引用的对象；已撤销/到期凭据的 verifier、TOTP、Session key、其他用途、符号链接和损坏文件均保留。失败应先恢复主库/状态目录可用性后重试，不手工批量删除 secrets。输出只含回收数量，不含密钥或文件内容。

源码删除不授权删除运行中的旧库、用户 artifacts、备份或 Codex profile。不得将新 schema 直接应用到旧库；实际产品切换仍须完成只读导入、备份恢复和回滚演练。当前没有声称达到 RPO/RTO。

## 尚待完成的产品部署验收

研究/组合/交付 UI、Worker/MCP/Codex 真闭环、受信任 runtime 与 job 隔离、多 Alpha/共享资金、Paper/Live/Forward/Wake，以及完整恢复/迁移仍未完成。普通 PR CI 不携带生产秘密，真实受保护验收只运行经过审查的固定 Head。QZ 不持有 Broker 凭据或真实执行控制权。

### 完整迁移命令的提交边界

`cargo run --locked -p server -- migrate --application-role '<已创建的运行角色>'`
在一个专用连接/外层事务内运行完整领域与原生 session DDL、验证表合同并授予
运行角色 DML 权限，最后一次性提交。执行前停止应用写入并完成备份；这不是
零停机承诺。不再额外运行独立的 `PostgresStore::migrate()`。角色不存在、既有
session 表不兼容或任一授权失败时，不保留半次升级及 epoch 失效副作用。
已有数据/会话不会被删表“修复”。网络在 COMMIT 阶段断开时结果未知，应在
主库重连后通过原生迁移记录和权限复核，不直接宣称回滚或重复恢复备份。
