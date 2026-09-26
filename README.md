<p align="center">
  <img src="apps/web/public/icon.svg" alt="QuaZonai" width="96" height="96">
</p>

# QuaZonai

个人量化研究工作台。从研究假设出发，组织 Alpha 实验、组合回测和目标组合交付，保存输入、过程与结果。支持浏览器和 PWA，不发送真实交易订单。

[安装](#quickstart) · [使用](#usage) · [更新与恢复](deploy/docker/README.md#update) · [Agent Skill](#agent-skill)

<a id="quickstart"></a>
## 安装

需要 Linux x86_64、本机 Docker Engine、Docker Compose 2.20+、Python 3.10+、Git、systemd 和 cgroup v2。Worker 需要 glibc 2.36+、OpenSSL 3；不支持 Docker Desktop、远程 Docker、rootless 或 userns-remap。完整前提见[部署手册](deploy/docker/README.md#prerequisites)。

从 [GitHub Releases](https://github.com/zhengui666/QuaZonai/releases) 选择提供 `quazonai-deploy.tar.gz` 的版本，下载后执行：

```sh
mkdir quazonai-install
tar -xzf quazonai-deploy.tar.gz -C quazonai-install
cd quazonai-install
```

以普通用户启动：

```sh
loginctl enable-linger "$USER"
bash deploy.sh
```

打开 **http://localhost:8081**。默认安装目录为 `$HOME/.local/share/quazonai`。

部署包按版本清单从 GHCR 拉取应用、科学计算、Codex 和数据库镜像，并安装同源 Worker 与 Runtime 网关。宿主机无需安装 Rust、Node.js 或 Codex。[科学 Runtime 与数据目录](deploy/docker/README.md#scientific-runtime)需要另外配置。

<a id="usage"></a>
## 开始研究

在「设置 → Codex → ChatGPT Auth」登录 ChatGPT，按页面提示完成授权，再为研究员和审阅员选择模型与推理强度。登录信息保存在独立目录，应用更新后继续使用。

登记真实数据与可用 Runtime，创建项目并冻结研究问题、输入和预算，启动研究周期。在运行详情查看进度与失败原因，在 Alpha 与组合详情检查评估、历史价值曲线和交付记录。

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
