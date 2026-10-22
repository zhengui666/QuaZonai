<p align="center">
  <img src="apps/web/public/icon.svg" alt="QuaZonai" width="96" height="96">
</p>

# QuaZonai

个人 AI 量化研究工作台：idea → 研究与独立评价 → 策略组合与共享账户回测 → 实盘／模拟盘观测。使用 TypeScript、官方 Pi Durable、NautilusTrader 和 Clarabel；Rust 仅保留科学内核。QZ 只交付目标，不持有券商账户、不发送真实交易订单。

[安装](#quickstart) · [使用](#usage) · [更新与恢复](deploy/docker/README.md#update-and-recovery) · [Agent Skill](#agent-skill)

<a id="quickstart"></a>
## 安装

服务需要 Linux x86_64、Docker Engine、Docker Compose 2.20+、Bash 4.4+、curl、tar、awk、GNU coreutils 和 util-linux。无需宿主 Node.js、Python、Rust 或独立 Worker；完整前提见[部署手册](deploy/docker/README.md#install)。

安装最新**已完整发布的 dev 版本**：

```sh
sh -c 'f=$(mktemp) || exit; cleanup() { rm -f -- "$f"; }; trap cleanup 0; trap "exit 1" 1 2 3 15; curl --fail --location --proto "=https" --proto-redir "=https" --tlsv1.2 --output "$f" https://raw.githubusercontent.com/zhengui666/QuaZonai/dev/deploy/install.sh && bash "$f" "$@"' sh
```

指定版本在末尾加 `--version v<版本>`，已有目录加 `--directory /绝对路径`，仅安装远程 CLI 加 `--cli-only`。新入口随完整 Release 发布后生效；远程 dev 提交不等于镜像和安装包已经发布。

默认安装目录 `$HOME/.local/share/quazonai`，打开 **http://localhost:8081** 完成密码设置。部署包拉取应用、科学计算和 PostgreSQL 镜像，不在安装主机编译。Node 应用是官方 durable 数据库的唯一所有者，Docker 自动重启后接续原任务。

**旧 Rust／Worker 安装不支持自动跨架构升级。** 新管理器会在修改旧状态前拒绝；继续使用原部署包完成对账和备份。原数据库、迁移、密钥、journal 和历史记录不能删除后假装迁移成功。详见[旧安装兼容边界](deploy/docker/README.md#legacy-rust-installations)。

CLI 包包含官方 Node 运行时，支持 Linux、macOS Intel／Apple Silicon 和 Windows x86_64。Linux／macOS 安装到 `$HOME/.local/bin`，Windows 安装到 `%LOCALAPPDATA%\QuaZonai\bin`：

```sh
quazonai --origin http://localhost:8081 login
quazonai research models
```

HTTP 只用于可信网络；公网使用 HTTPS。登录保存原有连接配置，后续命令复用同一凭据，不自动降级 HTTPS。

<a id="usage"></a>
## 开始研究

工作台仅保留五页：**研究看板、策略组合、观测、设置、扩展**。在研究页选择已配置的模型与数据集并输入 idea；组合页查看真实共享账户回测曲线；观测页读取所绑定的实盘／模拟盘连接。

私有 `data/service.json` 登记数据集、显式策略政策、连接和官方 Extension；`data/research/provider.env` 保存 Provider 环境配置。凭据不进入研究正文、浏览器表单或日志。没有模型、数据或证据时明确失败，不使用演示数据补齐。宿主配置与恢复见[研究指南](apps/research/README.md)。

创建研究时，可在「公开论文检索主题（可选）」中逐行填写 1–3 条愿意发送给论文网站的主题。研究 Agent 会在这些主题内自动查询 Crossref、arXiv 和 Semantic Scholar，并保存论文元数据、可用摘要及支持／反对论点的笔记；不会从研究正文自动提取外发查询。留空时不进行新的论文检索，科学研究仍可继续。研究详情的「文献」标签展示来源链接、适用限制和检索失败；重复查看复用已保存结果。论文接口无需额外密钥，但需要宿主能访问相应服务。文献帮助形成假设，不代表已阅读全文、完成独立评价或证明策略盈利。

```sh
quazonai datasets
quazonai research start --file research-request.json
quazonai research list
quazonai research get <id>
quazonai research stop <id>
quazonai portfolio list
quazonai observation list
```

保留原始提交文件与 `requestId`；超时后用同一正文、同一身份重放原回执。HTTP 202 仅表示接收。官方 durable 负责重试、子任务、重放、取消和恢复；研究继续沿用原试验账本和预算。独立样本不反馈模型调参，计算成功与回测曲线都不代表盈利或获准实盘交易。

数据获取与原生准备见[来源插件](runtimes/data/source-plugins.md)和[历史数据准备](runtimes/data/README.md)。准备成功不代替来源许可、历史可得性或质量证据。开发与精简核心链路检查见[项目入口](.opensdlc/project.md#commands)。

<a id="agent-skill"></a>
## 通过 Agent 操作

Agent 环境准备 Git 和 Node.js 22.20+（含 npm/npx），然后安装服务操作 Skill：

```sh
npx skills add zhengui666/QuaZonai --skill quazonai
```

Skill 操作已有服务，不负责开发或部署；见[连接说明](skills/quazonai/references/connection.md)。

## 许可

[AGPL-3.0-only](LICENSE) · [NOTICE](NOTICE) · [第三方声明](THIRD_PARTY_NOTICES.md)
