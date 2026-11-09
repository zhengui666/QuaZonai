from pathlib import Path
import json

out = Path(__file__).parent
# Reuse the previously checked ABI setter; do not execute its historical builder.
setter = (out.parent / 'planned-flow-probes/F06.wat').read_text().split('(global $init')[0]
wat = setter + '''(func $positive_finite (param $v f64)
 (if (i32.eqz (i32.and (f64.gt (local.get $v) (f64.const 0)) (f64.lt (local.get $v) (f64.const inf)))) (then unreachable)))
(global $count (mut i32) (i32.const 0))
(global $next (mut i32) (i32.const 0))
(func (export "qz_predict_v2") (param i64 i64 i32 i32 i32) (result f64)
 (local $i i32) (local $sum f64) (local $dt f64)
 (loop $masks
  (if (i32.ne (i32.load (i32.add (i32.const 64) (i32.mul (local.get $i) (i32.const 4)))) (i32.const 0)) (then unreachable))
  (local.set $i (i32.add (local.get $i) (i32.const 1)))
  (br_if $masks (i32.lt_u (local.get $i) (i32.const 8))))
 (local.set $dt (f64.load (i32.const 48)))
 (call $positive_finite (f64.load (i32.const 0)))
 (call $positive_finite (f64.load (i32.const 16)))
 (call $positive_finite (local.get $dt))
 (f64.store (i32.add (i32.const 128) (i32.mul (global.get $next) (i32.const 8))) (local.get $dt))
 (global.set $next (i32.and (i32.add (global.get $next) (i32.const 1)) (i32.const 31)))
 (if (i32.lt_u (global.get $count) (i32.const 32)) (then
  (global.set $count (i32.add (global.get $count) (i32.const 1)))))
 (if (i32.lt_u (global.get $count) (i32.const 32)) (then (return (f64.const 0))))
 ;; Recompute the short window so an expired huge gap leaves no subtraction residue.
 (local.set $i (i32.const 0))
 (loop $sum_window
  (local.set $sum (f64.add (local.get $sum) (f64.load (i32.add (i32.const 128) (i32.mul (local.get $i) (i32.const 8))))))
  (local.set $i (i32.add (local.get $i) (i32.const 1)))
  (br_if $sum_window (i32.lt_u (local.get $i) (i32.const 32))))
 (if (f64.gt (local.get $sum) (f64.const 60000000000)) (then (return (f64.const 0))))
 (f64.neg (f64.div (f64.sub (f64.load (i32.const 0)) (f64.load (i32.const 16))) (f64.load (i32.const 16)))))
)
'''
(out / 'L01.wat').write_text(wat)
(out / 'models.json').write_text(json.dumps({'L01': wat}, indent=2) + '\n')
previous = json.loads((out.parent / 'flow-memory-study-request.json').read_text())
goal = '''在完整登记的523个Hyperliquid现货与永续市场及既定443日输入中，沿用原训练/验证/最终独立分区和60秒事件时间目标，仅提交一次L01单因子：原C03回补分数乘因果成交活跃度准入。每品种从共同8/32价格EMA预热后第一次真实qz_predict_v2调用起，独立记录最近32个完整正成交簇间隔；不足32个间隔输出策略零分；T32=sum(last32 elapsed_event_ns)，T32<=60,000,000,000纳秒时输出−(cohort_vwap−fast_ema)/fast_ema，否则输出策略零分。恰好60秒准入；阈值与32窗口固定，不扫参数，不按后验资产/日期筛选，不删除无法评分的标识。零分是明确不承担风险的策略选择，不是补造缺失价格。FEATURE_MODEL_ABI_V2八槽mask=0可用；任一mask1/2/3须在修改环形状态前失败，不得把缺失输入记为零。实际使用的价格p、fastEMA和间隔dt须正且有限，在修改环形状态前检查；输出有限。逐簇状态不重采样、不虚构预热前31个间隔；不使用未来成交、未来退市状态、账户/订单身份。仅研究过去交易活跃时原回补机制能否保留预测信息，不能把过去成交活跃度当未来可成交/退出保证。无成交期间WASM不被调用，旧分数可能继续存在；目标零仍依赖之后真实成交、参与率与最小下单，不能抹除dust或估值未知。只读训练/验证反馈，保留全覆盖、零分比例、不可计算原因和标签实际跨度，不调用factor_finalize，不查看最终独立结果选参数。网络抖动0.1–1.5ms仍是回测假设，事件时间proxy不证明实际接收时钟PIT。原目标保持几千USDT、牛熊震荡费用后夏普>5且年化>150%；IC不等于费用后收益。本草稿不授权提交；root在现有四项context-v3经济结果后决定是否admit，届时核对共享100次原预算。本批若提交maxTrials=1且失败计数，不免费重试、不增加总额。保留全部既有试验和失败，当前已登记12/100；仅在该记录仍成立时本批累计最多13/100。不得修改登记总体、分区、期限、估值时效、费用/容量或编造价格。以下WAT为唯一预定实现，按原文调用一次factor_evaluate后停下；不把因子计算或零分比例当投资目标达标。\n\nL01 model:\n''' + wat
request = {
    'requestId': 'hyperliquid-liquidity-L01-draft-20261029-04',
    'goal': goal,
    'model': previous['model'],
    'thinkingLevel': previous['thinkingLevel'],
    'datasetId': previous['datasetId'],
    'maxTrials': 1,
    'specification': {
        'version': 1,
        'name': 'Hyperliquid C03因果成交活跃度 L01',
        'question': goal,
        'rationale': '原始目标（摘要）：Hyperliquid几千USDT因子、回测与组合在牛熊震荡费用后夏普>5且年化>150%。来源：owner-intent.json与search-design.md。当前C01 context-v3大部分日净值因持有现货缺少1小时内估值而未知；这是发现退出与估值风险的主要训练窗口诊断，不是最终独立反馈。Agent操作化：仅检验一个基于过去32间隔/60秒既定尺度的成交活跃度条件，保持全523范围。机制猜想：短期有连续交易活动的回补机会可能比稀疏跳价更可交易；过去活动不保证未来退出，最小订单残值与资金/context缺口仍可能令账户不可计算。与C03保留原符号与全部历史失败，不根据FUND/APU/ANZ名单排除资产。最终是否改善退出风险只能由真实共享账户经济比较回答，不由IC推断。',
        'exclusions': '不删资产/日期、不扫窗口阈值、不翻符号、不延长估值期限、不插值或补价、不改变退出/最小订单政策；不提交账户或最终选择，不把零分当缺失数据替代。',
        'evaluation': previous['specification']['evaluation'],
        'search': previous['specification']['search'],
    },
}
(out / 'research-request.draft.json').write_text(json.dumps(request, ensure_ascii=False, indent=2) + '\n')
