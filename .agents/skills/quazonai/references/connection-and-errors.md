# 连接、输出与错误

## CLI 与实例

使用现有 `quazonai` 可执行文件。发布版 CLI 自带 Node；直接使用 JavaScript 包需要 Node 22.19 或更新版本。

缺少可执行文件时，使用项目发布包中的官方安装器。在 Linux/macOS 只安装客户端时执行 `bash install.sh --cli-only`；Linux 默认路径还会安装或更新服务栈，因此不要省略这个选项。Windows x86_64 使用发布包的 `install.ps1`。安装完成后重新运行 `quazonai --version` 与 `quazonai --help`。不要假定 npm 包已发布，也不要用源码构建替代用户所需的客户端安装。

先运行 `quazonai identity`。正常会话返回 `authenticated: true`；未认证结果可能仍输出 JSON，但退出码为 `1`。

首次登录由用户在自己的终端执行：

```sh
quazonai login --origin https://qz.example.com --name my-agent
```

将示例域名替换为真实实例源地址。源地址只含协议、主机和可选端口，不附 `/api/v3`、用户名、密码、查询或片段。CLI 自己添加 `/api/v3`。

- 密码从交互 TTY 隐藏输入。不要索取聊天中的密码，不通过 stdin 管道或命令行参数传密码。
- 已有有效登录时 `login` 复用当前会话；切换实例或更换登录用 `login --origin URL --replace`。普通命令上的 `--origin` 不能切换已有配置。
- 自定义 CA 用 `--ca-certificate /absolute/path/ca.pem`，登录时保存的路径可复用；不要关闭 TLS 校验。
- `set_instance_password_in_auth_ui` 表示实例尚未设置密码，需用户在实例认证页面完成设置后再登录。
- 无 TTY 的 Agent 遇到 `login_requires_user_terminal` 时，提供上述登录命令，等待用户在同一执行环境完成后重新运行 `identity`。

配置保存在 Unix 的 `$XDG_CONFIG_HOME/quazonai/client.json`，未设置 XDG 时使用 `~/.config/quazonai/client.json`；Windows 使用 `%APPDATA%\quazonai\client.json`。其中包含设备 token，不读取、复制或输出它来完成普通业务操作。

| 命令 | 结果 |
| --- | --- |
| `logout` | 撤销当前远端设备凭据，成功后忘记本地配置 |
| `forget` | 仅删除本地配置，输出 `remoteRevocationRequested: false` |

`logout` 失败时不要以 `forget` 冒充已撤销凭据。

## 输出与退出码

| 场景 | 退出码 | 判读方式 |
| --- | --- | --- |
| 普通命令成功 | `0` | 读取 JSON 业务状态；提交回执只说明受理 |
| 输入、认证、HTTP 或连接失败 | `1` | 读取 stderr 的 `{error, status?}` |
| `identity` 未认证 | `1` | 读取 stdout 的会话字段 |
| 研究观察到 submission `done` | `0` | `settled: true`；继续检查研究结果 |
| 研究观察到 submission `unanswered` | `2` | 已结束但非成功提交；查看 diagnostics |
| 组合观察到 `COMPUTED` | `0` | `settled: true`；检查指标、限制和容量 |
| 组合观察到 `FAILED` / `STOPPED` | `2` | 读取业务错误和已有结果 |
| `wait` 达到观察超时 | `124` | `settled: false`、`cancellationRequested: false` |
| `watch` 达到观察超时 | `0` | 同样可能 `settled: false`，不能只看退出码 |
| `watch` / `wait` 被 SIGINT/SIGTERM 分离 | `130` | 不等于远端任务已取消 |

按行解析 `watch` 输出，不把整个 NDJSON 当成一个 JSON 对象。普通请求没有默认网络超时；外部终止写命令时，结果可能未知，应按未知写入处理。

## 错误恢复

| 错误或状态 | 操作 |
| --- | --- |
| `login_required` | 由用户完成交互登录，再读取身份 |
| `login_replace_required` | 核对目标实例；需要更换时重新登录并使用 `--replace` |
| `client_profile_invalid` | 保留错误信息；确需重置时用 `forget` 后重新登录，不手改 token |
| `invalid_origin` | 使用源地址，去掉路径、嵌入凭据及查询 |
| `service_unavailable` / `invalid_response` | 核对实例连接，恢复后重试读取；不重新创建业务对象 |
| `event_stream_unavailable` / `invalid_event_stream` | 用同一 ID 执行 `get`；连接恢复后重新 `watch` / `wait` |
| `write_outcome_unknown_retain_original_request` | 保留原文件与 ID，先查询结果，禁止自动生成新请求重试 |
| `request_id_conflict` | 同一 ID 对应了不同内容；找回原请求或明确创建新操作 |
| `mandate_revision_conflict` / `capital_exit_revision_conflict` | 读回所属对象修订号，重新判断操作；不盲目递增 |
| `file_required` / `input_file_unreadable` / `input_json_invalid` | 使用存在且可读的 JSON 对象文件，不能是数组、裸字符串或 stdin 的 `-` |
| `invalid_json_number_use_decimal_string` | 按合同将大整数/十进制改成字符串，保留真实精度 |
| `schema_not_found` | 从同一版本的 `schema list` 或 `schema list --domain` 选名称 |
| `installed_schema_unavailable` | 报告安装缺少合同资源，修复客户端安装；不猜请求结构 |
| `404` | 核对实例及 ID；删除隐藏的资源也可能无法读取 |
| `409`、业务 blocker、`UNSUPPORTED` / `BLOCKED` | 读取原因和最新状态；不要循环提交或扩展操作范围绕过限制 |

不能由 CLI 完成的首次实例配置、模型凭据配置、账户连接创建或数据源准备，应明确指出缺少的前置条件，并指向实例现有界面。不要虚构 `connections`、`models login`、`datasets import` 或 `request` 子命令。
