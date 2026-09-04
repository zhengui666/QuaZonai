# QuaZonai 用户运行操作模型

> 本文件是 [DESIGN.md](DESIGN.md) 的用户运行视图，不定义新的产品状态或技术
> 事实。遇到冲突时以 DESIGN.md 为准。

## 1. 正常旅程

QuaZonai 是单用户、自托管工作台。正常研究只有两类常规人工动作：

1. 提出 Idea，并回答最多一轮、最多三项真正改变研究边界的澄清；
2. 审批系统推荐的 Paper Candidate，或在 `MANUAL_APPROVAL` Mandate 下审批 Live
   Candidate。

目标闭环（不是当前已验收的 E2E）：

```text
Idea Draft → Clarification → frozen Charter → Research Cycle / fixed Mission DAG
→ PIT / evidence checks → qualified Alphas → multi-Alpha target portfolio
→ target-only Package → Paper Handoff → Forward Evidence
→ Live promotion or degradation Wake / Replan
```

当前已验证的是 Draft/固定 DAG、PIT Alpha signal、target-only Package archive 和纯
Promotion/Degradation policy；Package-before-Approval、Paper→Live 与自动 Wake/Replan
仍须独立 E2E 验收。

不是正常研究步骤的低频管理包括：首次认证、Universe、数据、Codex、Mandate、
Capital Context、Risk/Cost/Capacity、Promotion Policy、Paper/Live downstream 配置，
以及故障处置。

## 2. 责任边界

### Research Operator

- 提交 Idea Draft；
- 回答系统提出的边界问题；
- 查看 Program、Cycle、Mission、证据与阻塞原因；
- 在显示完整事实后作出 Paper/Live 审批或拒绝决定。

Operator 不手工选择 Alpha、调整权重、重写 Charter、操纵 Portfolio 或管理 Codex
Thread。

### Administrator

- 配置研究所需的 Universe、Data、Codex、Mandate、Capital Context、Risk/Cost/
  Capacity、Promotion Policy 和逻辑 downstream；
- 维护可用性、存储、worker、受信运行时与访问边界；
- 处理明确的 `ACTION_REQUIRED`；
- 在必要时 Pause、Resume 或 Archive Program。

### QuaZonai Automation

自动创建有界 Cycle 与固定 DAG：

```text
PLAN_RESEARCH → DATA_QUALITY → ALPHA_DISCOVERY → ROBUSTNESS
→ PORTFOLIO_ASSEMBLY → SEALED_PROMOTION_REVIEW
```

当前已验证的自动化创建固定 DAG 并保留 Mission 事实；PIT/质量、Alpha、Portfolio、
Promotion 与 degradation 的合同/持久化切片按各自实现推进。不要把这些切片描述为已
保留 Forward Evidence 或自动 Wake Event 的完整闭环。自动化不会把数据质量故障伪装成
Alpha 失败，也不会把一个 Agent 文本当成领域事实。

### Independent downstream

下游拥有 Paper/Live runtime、broker/market connectivity、订单、成交、仓位、账户、
NAV、执行风险与恢复。QuaZonai 只交付目标组合和接收约定的 Forward Evidence；它不
启动、停止、撤单、平仓或控制下游。

## 3. Idea Draft

用户提交自然语言 Idea 后，系统创建 `IdeaDraft`，不立即创建 Program。它只询问会
改变 Charter 的问题，例如市场范围、horizon 和数据范围。答案进入冻结 Charter；
完整答案后才出现 Start。

```text
DRAFT → ANSWERING → READY_TO_START → STARTED
```

Start 创建 Program、首个 Cycle 与持久 Mission 图。没有“预览后直接建 Program”、
手工 overlap 选择或不断修改已冻结 Charter 的常规路径。需要不同问题时创建新的
Draft/Program，而非原地改写历史事实。

## 4. 自治研究

`ACTIVE` Program 自治推进固定 DAG。每个 Mission 都有独立 Session、durable Codex
Thread 和临时 worktree；worker 中断先记录 `INTERRUPTED`，可恢复时继续同一 Thread。
用户看到的是可验证的结果、artifact、工具调用和状态，不是隐藏推理。

常见正常状态：

- `ACTIVE`：有可运行工作；
- `COOLING`：等待新的信息或证据；
- `WAITING_FOR_FEEDBACK`：等待有效 Forward Evidence；
- `BLOCKED`：缺少明确能力或出现需管理的故障；
- `PAUSED` / `ARCHIVED`：不运行自动工作，但保留历史和 Wake Event。

Mission 或数据失败必须分类为数据质量、运行时、Sealed evaluator、下游反馈或负面
研究证据；不能只用一个笼统的失败状态。

## 5. 数据、Alpha 与 Portfolio

Dataset 必须明确 `DISCOVERY`、`VALIDATION`、`SEALED` 或 `FORWARD` 用途，以及
`SYNTHETIC`、`FIXTURE`、`VENDOR` 或 `PRODUCTION` 来源。`event_time`、
`available_time`、`ingested_time`、PIT 和质量结果是正式事实。Synthetic/Fixture
默认不能晋级。

Alpha 只输出有限、PIT-valid 的 signal frame：`event_time`、`available_time`、
`instrument_id`、`score`，以及可选的已校准 `expected_return`/`uncertainty`。未校准
score 不是 expected return，也没有订单能力。

Portfolio Engine 是唯一的权重写入者。默认至少需要两个合格 Alpha；无法满足历史、
风险、成本、容量或约束时返回带原因的 `INFEASIBLE`，绝不降级为单 Alpha 的 100%
权重。`TargetPortfolioFrame` 是唯一 Package payload；独立 simulation、Paper 或 Live
推进仍需单独的持久化 Gate/E2E 证据。

## 6. Approval、Handoff 与反馈

目标顺序为（尚非已验收的持久化事务）：

```text
Candidate → target-only Package → independent validation → Approval → Handoff
```

当前 target-only archive 与纯 policy 已落地；预先持久化 Package、验证后绑定 Approval
及其 stale 处理仍待验收，不能由 archive 生成行为冒充。

Paper 与 Live 的分离和完整有效的 Paper evidence 才可进入 Live 的规则仍是产品边界；
`AUTO_HANDOFF`、Live Handoff 和 Forward Evidence 到 Promotion 的持久化闭环尚未验收。
Feedback 缺失、迟到或部分到达不等于 Candidate 失败。

下游领取后，QZ 不再提供运行时 revoke/stop。现有 degradation policy 不等于已持久化
advisory/Wake；即使未来形成 advisory，也不能替下游调仓或停止交易。

## 7. Pause、Resume、Archive 与 Wake

- **Pause**：停止新的自动工作；保留全部事实，不影响下游。
- **Resume**：从当前事实重新判断可运行工作；不重置 Search Ledger、Exposure 或
  已消费的 Episode。
- **Archive**：退出活跃池；不再自动研究或生成新的 Approval，历史仍可读。
- **Wake**：显式 lifecycle request 已存在；由新数据、Forward Evidence 或 degradation
  自动产生并创建 Replan 的闭环尚未验收。任何未来 Wake 对 Paused/Archived Program 都
  必须保持待处理，不能绕过人工状态。

这些操作需要当前 revision，避免把旧页面或旧命令覆盖较新的状态。

## 8. 访问与可见性

启用认证时，浏览器使用 Google Authenticator-compatible TOTP；CLI/automation 使用
独立 `QUAZONAI_API_TOKEN`。不要在聊天、日志或截图中暴露 setup secret、动态码、
cookie、machine token 或 downstream service token。只有受信 proxy 的准确 CIDR
可以转发来源信息；公网暴露需要 HTTPS 和部署侧访问控制。

普通研究页面不展示数据库行、lease、worktree 路径、Codex Thread ID、MCP transport、
Sealed raw data、Secret 或隐藏推理。它们只在必要的安全管理诊断中以受控方式出现。

## 9. Mobile Web / PWA

桌面、移动 Web 与已安装 PWA 共用同一个客户端和业务事实。PWA 只缓存静态壳，
`/api/**` 为 NetworkOnly；离线时明确显示服务不可用，不伪造研究数据或 mutation 成功。
发现新版本后由操作者确认更新。
