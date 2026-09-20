"""Apply only the web-authored documentation addition; no generated product logic."""
from pathlib import Path
import sys

root = Path(sys.argv[1])
path = root / "DESIGN.md"
original = path.read_text()
section = """

## 组合历史回测价值曲线

组合候选的已发表 PORTFOLIO 评估提供只读的模拟总权益曲线。价值来自该评估原 Run 已采纳的 `qz.portfolio_study` 中 `simulation.canonical_result.portfolio_snapshots[].total_equity`，时间为原生 `ts_event`。`qz.portfolio_history` 仍仅表示目标权重，不能充当权益。Nautilus 继续拥有估值、成交和费用；本功能不新增账本、回测、优化器、实时账户或基准引擎。

### 读取合同

`GET /api/v2/evaluations/{id}/equity-curve` 复用既有已发表候选评估与 Operator/CLI 读取规则，只接受 PORTFOLIO。查询为可选的含端点 `start_ns`、`end_ns`（DbCounter 十进制纳秒字符串）及 `resolution=AUTO|NATIVE|DAY|WEEK|MONTH`，默认 AUTO；未知字段、非法时间或逆序范围拒绝。

响应 `EquityCurveV1` 固定包含 `schema_version`、`project_id`、`candidate_id`、`evaluation_id`、`run_id`、`origin` 和 `curve`。`curve.status=READY` 时返回 `source_artifact_id` 及 `series`；`UNAVAILABLE` 时仅返回明确的 `reason_code`。序列包含基础币种、原生版本、原始起始资金、完整回测期间、实际显示粒度、原始/区间点数及最多 10000 个点。每点保存原始 `timestamp_ns` 和 DecimalValue 金额；如存在明确缺失点，金额为 null 并带原因，不补 0。

读取只关联原始终态 Attempt 已采纳的模拟产物，验证项目、候选、评估、Run、产物生产者和币种。浏览器不能指定任意产物或路径，也不会收到整个 canonical report、模型、训练索引、标签或 Sealed 结果。读取不启动任务、不改变资格和审批。科学 REJECT、历史有效期已过不阻止查看已发表的有效模拟；失败、无模拟、旧结果无快照或证据无效分别显示不可用，不反推虚构历史。

### 展示适配

AUTO 选择不超过 10000 点的最细可用粒度：原生、UTC 日、ISO 周、UTC 月。聚合仅选相应时间桶内最后一个真实观测；保留区间首尾原始点，报告实际粒度与点数。明确选择原生或其他粒度仍超过上限时返回可操作的范围错误，不静默截断。范围裁剪不重设初始资金。排序按精确纳秒；完全相同的重复观测可折叠，冲突的同时间值在无法证明原生先后顺序时作为数据错误，不任意挑选。正常休市不被武断改成缺失；明确空值不可跨越连线。

金额只在绘制边界转为经过 finite 检查的 JS number；提示与明细始终使用原始十进制字符串。纳秒先按 BigInt 处理再转换图表坐标。0/负权益保留，默认非对数轴；不平滑、不跨空值、不重复扣原生费用，不以收益率累乘替代原生权益。概览点不是指标计算输入，日/月末图不声称保留所有日内极值。

前端复用 Apache ECharts 的 Line、时间轴、DataZoom、Tooltip、Canvas 与按需导入，以及 echarts-for-react/core 的生命周期与 resize；只编写本业务的数据和样式配置。控件、明细表和反馈使用官方 antd，跟随 ConfigProvider token，无独立图表主题系统。图在候选评估指标表之前，保留必要币种、UTC/粒度及来源状态，不增加解释性文字墙。日期范围、恢复完整区间与明细支持键盘和触屏；390/768/1440 宽度可用。接口不进入 PWA 持久缓存；切换评估/区间取消旧请求并防止串图。

### 验证边界

覆盖真实 Nautilus 快照到领域投影、已采纳 Store/HTTP 读取与浏览器显示；检查持仓估值及费用而非现金替代，精确金额/纳秒、日周月边界、0/负值、单点、乱序/冲突、无数据、超限、过期/REJECT、错误来源及不可披露结果。React StrictMode、抽屉隐藏/显示、尺寸变化、主题、快速切换、失败/离线/重试、键盘与触控均验证。使用有界大数据夹具记录项目自身性能，不以库的宣传基准替代。代码、生成合同、依赖锁与用户文档共同交付；最新 Head CI、明确干净的 Codex review、合并后 main 核验仍是完成条件。
"""
if "## 组合历史回测价值曲线\n" in original:
    raise SystemExit("Design section already exists; inspect before editing")
path.write_text(original.rstrip() + section + "\n")
print("Updated DESIGN.md with the exact web-authored equity-curve contract")
