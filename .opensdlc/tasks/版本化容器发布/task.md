# 版本化容器发布

## 目标与范围

[PR #110](https://github.com/zhengui666/QuaZonai/pull/110) / [Issue #111](https://github.com/zhengui666/QuaZonai/issues/111)：版本 tag 的精确提交进入 main 后，自动构建并验证前后端生产镜像，发布到 GHCR 与 GitHub Release；部署包同时配置 PostgreSQL/PGMQ、网络、持久状态和同版本 Worker，并支持保留数据的更新。

字段及模块边界见 [DESIGN](../../../DESIGN.md#container-release)，命令见 [OPERATIONS](../../../OPERATIONS.md#container-install)。只修改独立工作树中的本任务文件，保留根工作树既有改动，不操作用户现有服务、数据库或认证，不自行选择正式版本 tag。源码、配置、脚本、测试和文档由网页端作者完成；执行器仅运行授权命令。

## 实现

| 模块 | 交付内容 |
| --- | --- |
| Dockerfile、入口和 Caddy | 前端构建产物、Rust release API、Caddy、锁定 Codex 与完整 sandbox resources 同镜像；多阶段构建，最终镜像不包含编译工具或测试数据。 |
| Compose | 应用和 PostgreSQL18/PGMQ，独立 bridge 网络、数据库持久卷、状态挂载、loopback 端口；Worker 从同镜像提取并交由宿主用户 systemd manager 运行，科学 Runtime 保持独立。 |
| Release 工作流与 release.py | 严格版本格式、轻量及附注 tag、main ancestry、精确 SHA 最新 CI；覆盖两种 tag/合并顺序，发布经过实际验证的同一镜像及其 digest，先上传 draft 附件再发布。 |
| manage.py、deploy.sh、update.sh | 首次安装、幂等重试、指定版本更新、静止点、备份、显式迁移、分阶段恢复与启动检查；不重建身份或密钥，不盲目回滚 schema，不删除数据卷。 |
| release_test.py、smoke.py | 本地逻辑/文件系统/故障注入回归与独立真实 Docker、systemd、PostgreSQL、原生沙箱、安装/升级/中断重试/恢复验收。 |
| README、OPERATIONS、DESIGN | 固定部署入口、必要主机条件、版本字段、运行边界与真实恢复动作；不把容器 liveness 等同于模型账号或科学 Runtime 就绪。 |

## 当前四项审查修复

基线提交 `5886877633ee818b3b97c759445053043d0852ea` 的七项 CI 已通过，但审查 `5307780023` 提出以下四项问题。工作区修复逐项对应，不复用旧 Head 的绿灯作为本次验收。

| Review comment | 修复与回归 |
| --- | --- |
| `4096425286`，首次安装在 current 创建前中断 | 迁移及原子 Worker 配置完成后，先持久化 `phase=starting`，再启动处理器；无 current 的 starting 重试仍跳过迁移、初始化和单元重写。旧无阶段记录在 DDL 前检查既有处理器。 |
| `4096425300`，关闭 API 时竞争准入的新 Run | 关闭 API 后、停止 Worker 前检查 Run；发现竞争准入时恢复原 API，保持原 Worker PID，不迁移。 |
| `4096425314`，恢复点未落盘 | 同步全部备份文件及目录，成功返回恢复点之后才持久化 migrating；同步失败不发布可用恢复点，不继续 DDL。 |
| `4096425324`，Worker 配置未落盘 | 环境文件和 unit 使用临时文件、文件 fsync、原子替换、父目录 fsync；两份配置完成后才 daemon-reload/start。同步失败不截断旧文件，也不启动候选服务。 |

更新也复用 starting 阶段：创建目标容器但不启动，持久化阶段后恢复原候选。启动/激活失败保留该阶段及已运行的处理器；同目标重试不再执行 DDL。没有新增发布服务、后台更新代理或单独恢复控制面。Linux 文件与目录的同步边界采用[原生 fsync 语义](https://man7.org/linux/man-pages/man2/fsync.2.html)。

## 本次实际验证

执行时间：`2026-09-24T18:10:55Z`。被测来源为基线 `5886877633ee818b3b97c759445053043d0852ea` 加本次五个工作树文件修改；本任务记录由网页作者在读取真实回执后更新，不改变被测程序。

| 命令 | 实际结果 |
| --- | --- |
| `python3 -B -m unittest discover -s deploy/docker -p '*_test.py' -v` | exit 0；56/56 通过，无失败 |
| `bash -n deploy/docker/entrypoint.sh` | exit 0 |
| `bash -n deploy/docker/deploy.sh` | exit 0 |
| `bash -n deploy/docker/update.sh` | exit 0 |
| `git diff --check` | exit 0 |

被测文件指纹：

```text
db89d7306a4ca6db69467ea2ff9a337cc39aa86a441d1ca96bfc9f52f0d87820  deploy/docker/manage.py
4de0a6ab3cccc15b24229512d84f5be0ddd5564ee8565aae224ddc3c19e875be  deploy/docker/release_test.py
7cf4346224b82ebd30f3b5c37ad0fc59c0625e89eea8a8c4375a99eeb318fe9c  deploy/docker/smoke.py
6e78dc315e43fb74ca4c058af48937d72778c23ec894f96e065f67cb943c6793  deploy/docker/Dockerfile
```

本次本地没有运行镜像构建、用户服务或数据库；smoke.py 在上述检查中只解析语法。新提交的真实容器安装、原生沙箱、启动前中断/重试保持 PID 与容器身份、关闭准入后的 Worker 保持、升级/故障恢复和 pg_restore 由 GitHub Container 工作流执行。对应 CI、独立 Review、线程处理和合并证据记录在 PR #110；检查未运行、仍在运行或旧 Head 成功均不记作新 Head 已通过。历史迭代结果保留在 Git 与 PR，不在此重复展开。

## 完成边界

1. 提出包含完整实现并关联 Issue #111 的 PR。
2. 相关 PR 当前 Head 的全部适用 CI 成功，所有有效审查问题已处理，且 `@codex review` 对该 Head 明确无问题。
3. 满足第 2 项后 PR merged 到 main，才完成代码交付。首个镜像实际发布另以版本 tag 的 Release workflow、GHCR digest 和 Release 附件为证据；工作流已写入或 PR 已合并不等于镜像已发布。
