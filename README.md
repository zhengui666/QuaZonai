<p align="center">
  <img src="apps/web/public/icon.svg" alt="QuaZonai" width="96" height="96">
</p>

# QuaZonai

个人、可扩展、无人值守的 AI 因子研究基座：研究目标与数据 → 因子提案及原生计算 → 样本外验证反馈 → 记录、迭代或停止。使用官方 Pi Durable 与 NautilusTrader，组合、模拟和目标交付为可选后续能力。支持浏览器/PWA 和 CLI，不发送真实交易订单。

[安装](#quickstart) · [使用](#usage) · [更新与恢复](deploy/docker/README.md#update) · [Agent Skill](#agent-skill)

<a id="quickstart"></a>
## 安装

需要 Linux x86_64、本机 Docker Engine、Docker Compose 2.20+、Bash 4.4+、curl、tar、awk、Git、GNU coreutils、util-linux、systemd（含 systemd-socket-activate） 和 cgroup v2。Worker 需要 glibc 2.36+、OpenSSL 3；不支持 Docker Desktop、远程 Docker、rootless 或 userns-remap。完整前提见[部署手册](deploy/docker/README.md#prerequisites)。

安装或更新最新**已完整发布的 dev 版本**，一行执行：

```sh
sh -c 'f=$(mktemp) || exit; cleanup() { rm -f -- "$f"; }; trap cleanup 0; trap "exit 1" 1 2 3 15; curl --fail --location --proto "=https" --proto-redir "=https" --tlsv1.2 --output "$f" https://raw.githubusercontent.com/zhengui666/QuaZonai/dev/deploy/install.sh && bash "$f" "$@"' sh
```

这条命令先完整下载脚本，下载失败时不会执行残留内容；源脚本从 GitHub Releases 选择最新、非草稿且完整上传的 dev 版本。指定版本可在末尾加 `--version v<版本>`；指定已有目录加 `--directory /绝对路径`。此 shell 入口需随新版本发布后才可使用；旧的已发布安装器不会自动变更。

首次安装前执行一次 `loginctl enable-linger "$USER"`。更新前完成运行并停止独立 Runtime；安装器保留原有数据、凭据、端口和恢复记录。

每次远程 `dev` 更新自动生成 `v<版本>-dev.<UTC时间戳>.<运行编号>`，通过该提交的完整 CI 后发布。每个 [Release](https://github.com/zhengui666/QuaZonai/releases) 的说明和 `README.md`、部署包内 README 都会自动写入**该次确切 tag** 的一行命令，复制即可安装或更新指定版本。

打开 **http://localhost:8081**。默认安装目录为 `$HOME/.local/share/quazonai`。

部署包按版本清单拉取应用、科学计算和数据库镜像，应用内携带官方 Node durable 宿主及同源科学入口，并安装匹配 Worker 与 Runtime 网关。宿主仅需 shell 与标准工具，无需 Python、Rust、Node.js 或 Codex，不运行编译或镜像构建。Release 同时提供三类镜像归档及 Windows、macOS、Linux 的原生 CLI 包。安装器保留原数据、备份与安装身份；[科学 Runtime 与数据目录](deploy/docker/README.md#scientific-runtime)仍可独立配置。

CLI 安装至 Linux/macOS 的 `$HOME/.local/bin` 或 Windows 的 `%LOCALAPPDATA%\QuaZonai\bin`。macOS/Windows 在 Release 中复制对应的一行命令；Linux 仅安装远程客户端时在该版本命令后加 `--cli-only`。Docker 集群运行于上述 Linux 主机。CLI 可直接连接明确指定的 HTTP 或 HTTPS 地址：

```sh
quazonai client --origin http://localhost:8081 login
```

HTTP 明文传输密码和令牌，请在可信网络中使用；公网连接使用 HTTPS。远程 HTTP 需先按[CLI HTTP 配置](deploy/docker/README.md#cli-http)设置服务端允许的确切地址；本机 localhost 无需额外配置。后续命令复用保存的连接，HTTPS 不会自动降级为 HTTP。

<a id="usage"></a>
## 开始研究

在安装的私有 research 配置中登记官方 Extension，并为 pi-ai 配置实际可用 Provider；凭据留在宿主环境，不进入研究正文或浏览器表单。没有模型时明确显示未配置，不使用旧 ChatGPT/Codex 登录或伪造目录。配置和原生计算边界见[研究宿主指南](apps/research/README.md)。

在「研究」选择实际模型，提供研究目标、原数据引用和评价/选择/停止政策，提交后无需逐实验审批。页面/CLI 观察同一 durable 会话、原生任务与用量，查看所有提案、失败、原生验证指标和停止原因。最终未接触样本只由独立评估扩展读取，结果不反馈模型调参；计算成功不等于有效证据或盈利。旧项目、Alpha、组合、运行和交付事实保留为历史或可选消费者。

```sh
quazonai client research models
quazonai openapi --schema DurableResearchStartV1
quazonai client research start --file research-request.json
quazonai client research list
```

保留原提交文件和 `requestId`，超时用相同内容重放原回执，不换编号重新研究。HTTP 202 只表示接收，不表示完成；显式停止使用 `quazonai client research stop <原生数字ID>`。

公开来源的获取、支持格式的原生转换和分区准备见[来源插件指南](runtimes/data/source-plugins.md)；Polymarket 历史来源与证据边界见[历史数据准备](runtimes/data/README.md)。准备成功不等于服务登记或研究准入，仍需对应的授权、历史可得性和质量证据；Coinbase 的当前用途限制见[来源使用边界](runtimes/data/source-plugins.md#source-rights-and-acceptance)。

右上角可切换浅色、深色主题。浏览器支持时可安装为 PWA；检测到新版本后按提示更新。

<a id="agent-skill"></a>
## 通过 Agent 操作

在 Agent 所在环境准备 Git 和 Node.js 22.20+（含 npm/npx），然后安装操作 Skill：

```sh
npx skills add zhengui666/QuaZonai --skill quazonai
```

Skill 操作已有 QuaZonai 服务；连接方式见[连接说明](skills/quazonai/references/connection.md)。

## 许可

[AGPL-3.0-only](LICENSE) · [NOTICE](NOTICE) · [第三方声明](THIRD_PARTY_NOTICES.md)
