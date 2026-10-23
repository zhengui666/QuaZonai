# PM 四平台 CLI 输出编码修复

## 目标与基线

继续完成 PM 平台的正式 Dev 发布。基线为最新远程 dev `b03d55e7e6c25f8846d6a42d2a40e236d4a77c21`，从该精确提交签出 `codex/pm-cli-utf8`。业务学习、资金方法、账户约束、完整科学检查和发布产物身份规则保持原样。

## 实际失败证据

- 基线的 [push CI 39886420490](https://github.com/zhengui666/QuaZonai/actions/runs/39886420490) 全部成功：release 科学 676/676（50组）、完整 core 91/91，零失败/取消/跳过；可选历史 Rust 59/59 和 Python 2/2，TypeScript/部署及 Web 26 单元、58 浏览器全部通过。science-core 用时 77分54秒，已经实际验证扩大后的执行预算。
- 同一次 [Dev release 39886420647](https://github.com/zhengui666/QuaZonai/actions/runs/39886420647) 的 checks 成功，Linux、macOS Intel 和 ARM 三个 CLI 均成功；[Windows job 120218706049](https://github.com/zhengui666/QuaZonai/actions/runs/39886420647/job/120218706049) 在实际解包执行 schema smoke 时失败。
- 2026-10-23T03:11:03Z，Python 3.12.10 的 `subprocess.run(capture_output=True, text=True)` 用 Windows CP1252 解码 Node 的 UTF-8 JSON，报 `UnicodeDecodeError`（byte 0x8d，position 47830）；读取线程失败后 `stdout` 为 None，原 `.strip()` 随后报错。ZIP 已生成，但 Windows 安装和产物上传未执行。
- 实际标签 `v3.0.0-dev.20261023015214.39886420647` 直接指向基线；有标签及三个平台通过不等于已经发布。该失败保留原运行、日志和范围，不通过重跑旧源或跳过 Windows 得到绿色。

## 修复与不变量

解包 CLI 的 stdout/stderr 捕获明确使用 `encoding='utf-8', errors='strict'`。UTF-8 是 Node 文本输出协议，不能依赖宿主默认代码页；不修改全局 locale，不采用 ignore/replace，不把损坏 JSON 视为成功。

原版本核对、帮助命令、PM 合同、来源恢复类型和原生 CASH 数值协议检查均保留。四平台真实运行、原生精确 revision/run/artifact 检查、schema4 manifest、固定镜像 digest 和 12 项正式资产要求均保留。没有金融参数、TTL、试验预算或科学方法改动。

## 验收与交付

本地官方 Node 22.19.0、Python 3.12.14/Linux 已实际执行：旧生产源码临时副本的 5 项测试中原有 2 项通过、新增 3 项失败；原 schema stdout 的 CP1252 错误精确复现 byte 0x8d / position 47830，真实 UTF-8 stderr 也复现。修复后 5/5 通过、零跳过，损坏 UTF-8 stdout 仍被拒绝。回归读取当前原始合同并实际执行整个 CLI smoke，只模拟公开的默认代码页查询，不替换子进程成功结果、不镜像 schema、不依赖外部 QA 归档或固定机器路径。前提是项目 Node 与已安装 CLI 依赖，缺少时明确失败。另有现存 release 8/8、官方 Node22 workflow 4/4、两份 Python AST 和 diff 检查通过。

以真实子进程 UTF-8 输出、CP1252 默认解码复现原路径的红绿回归；Linux 的代码页模拟不替代真正 Windows 安装验收。运行适用 package/release/workflow 检查，保留实际日志与结果。

最终 Head 需要全部适用 CI、独立精确 Head 审阅和 GitHub Codex 明确无问题，再合并远程 dev。合并后继续核验新 merge 的完整 push CI 及同次 Dev release，直到四平台 CLI、真实镜像安装/备份恢复、12 项非空正式资产及 manifest/tag/revision/digest 都实际通过。正式发布不代表切换了用户的现有安装。

业务交付范围和真实模型/经济容量/SHADOW 上限沿用 [PM 任务](../pm-adaptive-platform/task.md) 与 [运维边界](../pm-adaptive-platform/operations.md)。
