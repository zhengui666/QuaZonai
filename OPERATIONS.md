# 运行与部署边界

当前是Rust重写实施分支，不是完整可部署产品。旧后端、旧前端、旧Compose和旧插件平台已按所有者要求删除；不保留兼容服务。当前支持README的原生fixture、逐轮Store/新库测试命令，不承诺Web或真实研究启动。

## 数据与权限

源码删除不授权删除运行中的数据库、artifacts、备份、Codex profile或宿主用户数据。不要在旧数据库运行新schema；当前SQLx迁移仅针对新数据库，实际切换使用独立数据库/卷，并按DESIGN完成只读导入报告和回滚演练。

原生兼容性命令只处理其固定synthetic输入，输出目录必须不存在。不能把fixture标为REAL、生成生产可领取Package或以测试开关跳过Gate。进程stdout只表示兼容性，不表示完整隔离/业务正确性。

## 待完成的部署验收

server/worker/PostgreSQL+PGMQ、受信任runtime与隔离job、同源HTTPS、首次本机bootstrap/TOTP、原生Codex登录、授权数据、目标交付、备份/恢复仍按DESIGN验收。没有部署命令就不在此写一键启动。没有实际演练不报告达到RPO/RTO。

GitHub普通PR CI不携带生产秘密；受保护账号验收只能运行经过审查的固定Head。QZ不持有真实Broker凭据、订单/NAV账本或停止交易权限。
