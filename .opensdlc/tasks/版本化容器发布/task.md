# 版本化容器发布

## 目标

版本号 tag 指向已合并 main 的提交时，自动构建前后端生产镜像，发布到 GHCR，并在 GitHub Release 提供部署包。部署脚本同时部署应用、PostgreSQL/PGMQ、持久存储和网络；更新保持数据、密钥、Codex 原生会话及任务身份。

## 基线与实现边界

基线 main：`d6bddbbd40e559ba86b6dffcf140289b15d049d9`。使用独立分支，不改本机并发工作区，不操作既有数据库、服务或进程。

- 同一镜像打包 Rust release server、构建后的前端、Caddy 和仓库 lockfile 对应的官方 Codex。
- Compose 显式 bridge 网络；应用和数据库仅发布宿主机 loopback 端口。PostgreSQL 18 数据卷挂载 `/var/lib/postgresql`，复用现有 CI 验证的 PGMQ 镜像 digest。
- API 保留原有 loopback bind/PUBLIC_URL 约束；Caddy 与 API 在同一容器，网关从容器网卡接收本机映射的请求。
- 原生 Mission 需要 `/usr/bin/systemd-run --user`、真实 `/user.slice/` cgroup 与 `cgroup.kill`。不以特权容器或可写宿主机 cgroup 挂载冒充兼容。完整部署需从同一镜像提取 Worker/Codex 二进制，以现有用户的 systemd user service 运行；使用同一状态路径和原 CODEX_HOME，不复制 auth.json。
- 科学 Runtime、其 journal 和数据目录仍属于原有独立 Runtime 部署，不把空允许列表或容器存活当成科学链路就绪。

## 本轮变更

已编写 Dockerfile、定向 build-context ignore、容器入口、Caddy 网关、Compose 和隔离的 GitHub Actions 容器构建/API/真实数据库重启检查。该检查不使用真实模型账户，不执行付费推理，不测试原生 Worker 部署。

## 尚未完成

部署管理器的远程写入被平台拦截，因此没有提交 `manage.py`，也没有提交指向不存在管理器的 deploy/update 包装脚本。当前 PR 必须保持 Draft；现有 CI 通过也不代表以下需求已完成。

1. 发布筛选：严格 `vMAJOR.MINOR.PATCH[-prerelease]`；同时处理 tag 在合并前/合并后推送，通过 `git merge-base --is-ancestor` 检查精确 tag commit；普通 main push 不产生无版本发布。处理 annotated tags、重复推送和已发布版本，不把预发布覆盖为稳定版。
2. 发布流程：构建并验证精确 tagged source；PR 任务只读，发布任务才持有 `packages:write`/`contents:write`。推送 GHCR，并生成记录 image digest、source SHA、数据库 digest、版本的 release.json。部署按 digest 拉取，不依赖浮动 latest。
3. 完整安装：本机 Docker Compose、原生 systemd/cgroup 前置检查；随机数据库密码；独立安装目录；首次状态初始化和显式迁移；应用健康后启动同版本原生 Worker；中断后能保留密码和状态重试，不覆盖已有安装或服务。
4. 更新：先下载并校验指定版本包/拉镜像；记录并校验当前安装；没有未终结 Run 才停止应用和 Worker，停止后再次确认静止点；备份 PG、状态和单独的 master key；显式迁移；切换版本并真实检查。失败时不得盲目回滚 schema、清空卷、伪造任务终态或覆盖密码。新版本脚本和 Compose 必须随版本更新。
5. 验证：标签/主干条件与并发发布测试、真实 install→持久化→update→重启、迁移失败/缺密钥/重复安装/备份失败/活跃 Run 的故障路径；确认镜像和 Release 真正存在后才宣称已发布。
6. 文档：把已可执行的容器安装/更新命令接入 OPERATIONS.md 与 README，明确首次 GHCR 可见性/认证、Linux x86_64 支持边界和原生 Worker/独立科学 Runtime 的关系。

## 交付条件

全部实现提交后，当前 Head 的适用 CI 全绿、`@codex review` 明确无问题并合并 main。未经用户选择正式版本，不擅自创建首个 release tag。不使用未执行、旧 Head 或与该需求无关的绿灯作为完成证据。
