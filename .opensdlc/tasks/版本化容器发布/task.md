# 版本化容器发布

## 目标与合同

PR #110：版本 tag 指向已合并 main 的提交时，自动构建、验证并发布前后端生产镜像到 GHCR，附 GitHub Release 部署包。部署应用、PostgreSQL/PGMQ、持久存储和网络；更新保留数据、密码、master key、原生会话和任务身份。字段及运行边界见 [DESIGN](../../../DESIGN.md#container-release)，命令见 [OPERATIONS](../../../OPERATIONS.md#container-install)。

继续基线为 `5e63c5721b6e7ee8fa96b0a200c5b294c80676d9`；只修改独立 worktree，不混入原根目录未提交变更。不擅自创建正式版本 tag，不操作现有数据库/产品服务。

## 实现

- 修复官方 Codex 锁定包不存在 `path/*` 的打包错误，复用 bin 与发行版 ripgrep。前端产物、Rust release API、Caddy、Codex 同镜像交付。
- Compose 为应用/数据库建立独立 bridge 网络与持久状态，宿主端口仅 loopback。Worker 从同镜像提取并运行于原用户 systemd manager，保留 Mission cgroup 合同。
- release.py / release.yml 处理严格版本、轻量/附注 tag、main 祖先、exact SHA 最新 CI、两种合并顺序、重复发布和 prerelease。直接发布实际被测镜像，拉回 digest 比较；附件先进入 draft 后发布。
- manage.py / deploy.sh / update.sh 包含首次安装、同身份重试、指定版本下载、原生迁移、静止点、备份、失败停机/恢复、同目标继续、真实 HTTP/Worker 启动检查。密码不输出，安装/升级不自动删卷或重建 key；数据库升级保持独立。
- PR / Release 复用一个 Container composite，定向测试与真实安装→升级→故障→重试→重启→pg_restore 验证分开记录。生产代码没有 Demo/Mock 入口。
- README、OPERATIONS、DESIGN 与独立部署包 README 对齐。

## 已观察的执行

2026-09-24：CodexPro 读取工作区正常；嵌套 workspace selection 没有跨请求保留，改为默认根目录下的完整相对路径读写。新的完整部署管理器已实际写入，不再是缺失文件。

取证 handoff 请求 `gpt-5.6-luna`，执行从 06:04:56Z 到 06:08:43Z，exit 0；仅 Git/GitHub 读取和准备隔离工作树。读取到 main `0add52dc4cc92e70f971e4f4b6536032ec063c9b`，PR110 open/draft。旧 Head 六项 CI 成功，Container run `35943721872` 因 Dockerfile cp 不存在的 Codex path 目录失败；后续容器测试当时未运行。

源码、脚本、测试和文档由网页端作者完成。后续交接记录确认提交 `6eabed5de88f5611a6d6624c28668bce9a309f0a` 已推送 PR110 并关联 Issue111，13 项定向测试与三个 Shell 语法检查通过。

2026-09-24 07:24:50Z 的本轮取证执行器请求 `gpt-5.6-luna`，实际 adapter exit 0；PR Head 仍为 `6eabed5`，六项 CI 成功，Container run `35965508432` / job `107523083725` 在前端编译时因缺少 `tests/contracts/data-registry-keys.json` 失败。原生数据库部署验收在该运行中尚未执行。当前审查另指出 systemd 路径错误引用、首次安装端口检查和发布登录 action 的版本固定问题。

网页端已补全容器构建所需测试合同（不进入最终镜像）、按 systemd 原生单路径语义修复 WorkingDirectory/EnvironmentFile、在首次保存配置前检查端口范围/占用、固定 login-action v3.7.0 的确切提交。新增真实 socket 占用与路径回归测试，真实部署验收改用含空格、百分号和美元符号的路径；逐个检查 Shell 脚本而不是把后续文件误作第一个脚本的参数。上述新修改仍需以下当前源码验证：

2026-09-24 13:42:01Z 的实际 GitHub 回执确认 `c0f2986c2ea2600629abec14cba4a846cc93778d` 全部七项 CI 成功，包括 Container run `36000319770` 的安装/更新/恢复；Codex 评论 `5814340841` 对该 SHA 给出无新增问题结果。但四个旧审查线程尚未解决，原审查 SHA 是 `285f8f3`，不能把重新定位后的评论 SHA 误记为新的审查。

本轮补齐四项原缺陷：固定 composite 中两个 Docker action 的上游提交；保存安装身份前拒绝 systemd 不可执行的目录并在停旧服务前原生校验候选 unit；更新期间禁用 Worker，避免迁移失败后重启拉起旧版本；保留完整 codex-resources 并对提取出的官方 Codex 执行实际沙箱命令。回归覆盖路径/缺资源/原生校验失败、升级前置失败不触碰服务、升级失败 unit 已禁用。真实 smoke 同时覆盖带方括号的目录和无账号沙箱执行。仅修改本任务文件，主机原服务、数据库和根工作树未改。

以下清单针对上述新源码；`c0f2986` 的成功不替代新 Head 验证。本轮写入后尚未运行测试、提交或推送：

- [ ] Python 定向测试、Shell 语法、差异检查。
- [ ] 当前 Head 的 Container 构建、真实安装/升级/故障恢复/数据库恢复。
- [ ] 其他适用 GitHub CI。
- [ ] 当前 Head 独立 Codex Review 明确无问题，处理全部有效反馈。
- [ ] 条件满足后 merge main，并记录 exact merge commit。

## 完成边界

1. 完整实现提出 PR 并关联需求 Issue。
2. 相关 PR 当前 Head 全部适用 CI 通过，Review 问题解决且 `@codex review` 明确无问题。
3. 满足第2项后 PR merged 到 main 才完成代码交付。首个镜像实际发布另以版本 tag 的 Release workflow、GHCR digest 和 Release 附件为准，不虚报已发布。
