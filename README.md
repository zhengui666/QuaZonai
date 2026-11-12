# QuaZonai · Docker 部署

QuaZonai 是连接 NautilusTrader 与官方 Pi Durable 的个人量化工作台。应用容器提供 Web/API 和研究宿主；原生计算需要另外配置可访问的 Nautilus 实例及已注册数据目录。安装不会自动获得市场数据、配置账户或开始交易。

## 环境

- Linux x86_64，Docker Engine 与 Docker Compose 插件。
- Bash 4.4+、curl、tar 及常用 Unix 工具；安装用户应能运行 Docker。
- 可访问 GitHub Releases 与镜像仓库。不需要在宿主机安装 Node.js 或 Rust。

## 安装

从 [Releases](https://github.com/zhengui666/QuaZonai/releases) 选择包含完整部署资产的版本，下载该版本的 `install.sh`，然后执行：

```bash
bash install.sh --directory "$HOME/.local/share/quazonai" --port 8081
```

安装器使用发布的预构建镜像部署 Docker Compose，不在本机编译。默认访问地址为 `http://localhost:8081`，首次访问按页面设置密码。默认仅监听宿主机回环地址；远程使用可通过 SSH 转发访问，不要直接向公网暴露未配置的服务。

在设置中配置模型提供方、Nautilus 回测连接和数据集；模拟盘与实盘连接分别配置，不自动互相替代。提供方密钥及连接 token 所引用的环境变量保存在安装目录的 `data/research/provider.env`，不要填入研究提示词或提交到仓库。应用镜像本身不等于已连接原生执行服务。

## 当前使用范围

可以在研究会话中配置并执行原生指标历史回测、继续调整参数，以及从原生研究结果发起参数扫描。指标预热、执行与统计复用 Nautilus；任务、结果和恢复复用 Pi Durable。当前信号接线为 EMA 交叉、RSI 和 MACD，并可查询 ATR、VWAP、Stochastics、Donchian 观测。它们不是 Nautilus 的全部能力；尚不支持任意 Python 策略动态导入。

扫描使用原生统计比较历史结果，不是自动选出可盈利策略，也不会自动开始模拟盘或实盘。旧自研评分、因子拟合和 PM 科研创建流程已退役，不提供旧流程兼容运行。条件订单当前仅提供原生预览；新 paper/live bracket 均未开放执行。

## 更新与数据保留

更新时下载目标发布版本的 `install.sh`，使用相同安装用户和原 `--directory` 再次执行。保留完整安装目录、`installation.json`、数据、凭据和原版本发布包；不要以删除目录或数据库的方式解决升级问题。备份、恢复及启停命令使用该版本部署包内 `manage.sh --help` 所列方式。备份会停止应用，完成后需显式启动。

容器重启不等于数据删除，也不等于远端任务已结束。保留应用挂载目录和独立 Nautilus 数据/账户目录，未决订单与执行回执须继续核实。旧安装格式、旧科研任务及旧原生版本不承诺自动迁移；升级前查阅对应发布说明。
