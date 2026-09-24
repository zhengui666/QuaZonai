# 版本化容器发布

## 目标与范围

[PR #110](https://github.com/zhengui666/QuaZonai/pull/110) / [Issue #111](https://github.com/zhengui666/QuaZonai/issues/111)：版本 tag 的精确提交进入 main 后，自动构建并验证前后端生产镜像，发布到 GHCR 与 GitHub Release；部署包同时配置 PostgreSQL/PGMQ、网络、持久状态和同版本 Worker，并支持保留数据的更新。

字段与模块合同见 [DESIGN](../../../DESIGN.md#container-release)，实际命令见 [OPERATIONS](../../../OPERATIONS.md#container-install)。只修改独立工作树中的本任务文件，不混入根工作树的既有改动，不操作用户现有服务、数据库或认证。不自行选择正式版本 tag。

## 实现与审查修复

- 多阶段镜像包含前端静态产物、Rust release API、Caddy、锁定官方 Codex 及完整 codex-resources。Caddy 去除不需要的 file capability；容器验证使用与部署相同的权限条件，官方沙箱验证不调用模型或真实账户。
- Compose 管理应用、PGMQ 数据库、独立 bridge 网络和持久卷；Worker 从同镜像提取，在原用户 systemd manager 中运行。原生科学 Runtime 的数据目录和 journal 保持独立。
- 发布复用一个 Container composite，按 tag 精确 SHA 检查 main 祖先、最新适用 CI、同源镜像身份；覆盖合并前后两种打 tag 顺序，先上传 draft 附件再发布，不维护浮动 latest。
- 首次安装在持久化身份前检查端口、可表达的原生路径、Docker UID/GID 映射和该 Compose project 的遗留资源；缺原 manifest 的旧卷不能与新密码或密钥混用。
- 候选 unit 在停旧服务前经过 systemd 原生校验。更新检查静止点、备份并显式迁移，期间禁用 Worker 开机启动；迁移后失败保持停止/禁用，成功激活才启用。重试保留原恢复点。
- 更新拒绝 SemVer 降级，同版本保持幂等；成功输出发生断管不再停止已激活服务。真实回归包含遗留卷拒绝、降级不改变 Worker PID、沙箱、升级失败重试、数据库恢复及密钥保持。

## 已观察的验证事实

| 来源 | 实际结果 |
| --- | --- |
| `c0f2986c2ea2600629abec14cba4a846cc93778d` | 七项 GitHub CI 成功；Codex 评论 `5814340841` 对该提交给出无新增问题反馈。此结果不替代后续提交的审查。 |
| `348bae80a2f417e11c93034b14620f149e7ca72f` | 提交前 21 项 Python 测试、三个脚本的独立语法检查及差异检查通过，已实际提交并非强制推送。Container run `36008645043` 已成功执行安装、沙箱、更新、失败重试和恢复。 |
| 2026-09-24 14:14:04Z 的远端快照 | `348bae80` 六项 CI 成功，store-postgres 当时仍运行；当前提交审查 `5305487729` 提出新的输出、遗留卷、UID/GID 映射、任务记录及降级问题，不能记作 clean review。 |
| 2026-09-24 22:29:17 +08:00，`0988a1c3` 提交前的源码 | `python3 -B -m unittest discover -s deploy/docker -p '*_test.py' -v`：26/26 通过；分别 `bash -n` 检查 entrypoint.sh、deploy.sh、update.sh：全部 exit 0；`git diff --check`：exit 0。 |
| `0988a1c36b0f571e126fba370eef094c9c0e8c72` | Container run `36013871835` 成功；该提交审查提出停止 Worker 失败恢复、候选容器重启策略和 CODEX_HOME 重叠三项问题，未达到 clean review。 |
| 2026-09-24 15:11:13Z，本次提交前的源码 | 32/32 Python 测试通过；三个独立 `bash -n` 和 `git diff --check` 全部 exit 0。验证回执记录四个被测源文件的指纹；真实新增容器场景仍以该提交的 CI 为准。 |

本次已将停止步骤纳入迁移前恢复路径；应用待恢复时使用原生 `restart=no`，激活检查通过后才开启自动重启；解析真实路径后拒绝 CODEX_HOME 与托管目录的重叠。32 项单元测试覆盖 manage.py / release.py 的逻辑，包含上述恢复与布局回归，以及 SemVer 顺序/降级拒绝、遗留 Docker 资源、重映射 daemon 及输出断管；smoke.py 在本次本地检查中只解析语法，不记作真实容器执行；任务记录在读取该执行回执后更新，未由执行器代写产品文件。主机 Docker 构建不作为本地证据，实际容器验收在 GitHub Actions 执行。新 Head 的 CI、Review、线程处理和合并结果以 PR #110 的实际记录为准；旧 Head 的成功不跨版本复用。

## 完成边界

1. 完整实现提出 PR，并关联需求 Issue。
2. 相关 PR 当前 Head 全部适用 CI 成功，所有有效审查问题已处理，且 `@codex review` 对当前 Head 明确无问题。
3. 满足第 2 项后 PR merged 到 main，才完成代码交付。首个镜像实际发布以版本 tag 的 Release workflow、GHCR digest 和 Release 附件为独立证据，不将工作流已写入或 PR 已合并等同于镜像已发布。
