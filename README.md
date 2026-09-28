<p align="center">
  <img src="apps/web/public/icon.svg" alt="QuaZonai" width="96" height="96">
</p>

# QuaZonai

个人量化研究工作台。从研究假设出发，组织 Alpha 实验、组合回测和目标组合交付，保存输入、过程与结果。支持浏览器和 PWA，不发送真实交易订单。

[安装](#quickstart) · [使用](#usage) · [更新与恢复](deploy/docker/README.md#update) · [Agent Skill](#agent-skill)

<a id="quickstart"></a>
## 安装

需要 Linux x86_64、本机 Docker Engine、Docker Compose 2.20+、Python 3.10+、Git、systemd 和 cgroup v2。Worker 需要 glibc 2.36+、OpenSSL 3；不支持 Docker Desktop、远程 Docker、rootless 或 userns-remap。完整前提见[部署手册](deploy/docker/README.md#prerequisites)。

安装或更新最新**已完整发布的 dev 版本**，一行执行：

```sh
python3 -c 'import json,re,urllib.request,subprocess; releases=json.load(urllib.request.urlopen("https://api.github.com/repos/zhengui666/QuaZonai/releases?per_page=100")); r=max((r for r in releases if not r["draft"] and re.fullmatch(r"v[0-9]+\.[0-9]+\.[0-9]+-dev\.[0-9]{14}\.[1-9][0-9]*",r["tag_name"]) and any(a["name"]=="install.sh" for a in r["assets"])),key=lambda r:tuple(map(int,re.findall(r"[0-9]+",r["tag_name"])))); subprocess.run(["bash"],input=urllib.request.urlopen("https://github.com/zhengui666/QuaZonai/releases/download/"+r["tag_name"]+"/install.sh").read(),check=True)'
```

首次安装前执行一次 `loginctl enable-linger "$USER"`。更新前完成运行并停止独立 Runtime；安装器保留原有数据、凭据、端口和恢复记录。

每次远程 `dev` 更新自动生成 `v<版本>-dev.<UTC时间戳>.<运行编号>`，通过该提交的完整 CI 后发布。每个 [Release](https://github.com/zhengui666/QuaZonai/releases) 的说明和 `README.md`、部署包内 README 都会自动写入**该次确切 tag** 的一行命令，复制即可安装或更新指定版本。

打开 **http://localhost:8081**。默认安装目录为 `$HOME/.local/share/quazonai`。

部署包按版本清单从 GHCR 拉取应用、科学计算、Codex 和数据库镜像，并安装同源 Worker 与 Runtime 网关。宿主机无需安装 Rust、Node.js 或 Codex，不运行任何编译或镜像构建。Release 同时提供全部四类 Docker 镜像归档、SHA-256 校验和，以及 Windows x86_64、macOS Intel/Apple Silicon、Linux x86_64 的原生 CLI 包。[科学 Runtime 与数据目录](deploy/docker/README.md#scientific-runtime)需要另外配置。

CLI 安装至 Linux/macOS 的 `$HOME/.local/bin` 或 Windows 的 `%LOCALAPPDATA%\QuaZonai\bin`。macOS/Windows 在 Release 中复制对应的一行命令；Linux 仅安装远程客户端时在该版本命令后加 `--cli-only`。Docker 集群运行于上述 Linux 主机。CLI 可直接连接明确指定的 HTTP 或 HTTPS 地址：

```sh
quazonai client --origin http://localhost:8081 login
```

HTTP 明文传输密码和令牌，请在可信网络中使用；公网连接使用 HTTPS。远程 HTTP 需先按[CLI HTTP 配置](deploy/docker/README.md#cli-http)设置服务端允许的确切地址；本机 localhost 无需额外配置。后续命令复用保存的连接，HTTPS 不会自动降级为 HTTP。

<a id="usage"></a>
## 开始研究

在「设置 → Codex → ChatGPT Auth」登录 ChatGPT，按页面提示完成授权，再为研究员和审阅员选择模型与推理强度。登录信息保存在独立目录，应用更新后继续使用。

登记真实数据与可用 Runtime，创建项目并冻结研究问题、输入和预算，启动研究周期。在运行详情查看进度与失败原因，在 Alpha 与组合详情检查评估、历史价值曲线和交付记录。

Polymarket 公开历史来源、按需下载与原生目录转换见[历史数据准备](runtimes/data/README.md)；研究准入仍需对应的历史可得性和质量证据。

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
