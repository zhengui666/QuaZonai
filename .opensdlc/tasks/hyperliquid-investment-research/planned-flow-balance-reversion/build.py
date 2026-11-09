from pathlib import Path
import json

out = Path(__file__).resolve().parent
setter = (out.parent / 'planned-flow-probes/F06.wat').read_text().split('(global $init')[0]
wat = setter + '''(func $positive_finite (param $v f64)
 (if (i32.eqz (i32.and (f64.gt (local.get $v) (f64.const 0)) (f64.lt (local.get $v) (f64.const inf)))) (then unreachable)))
(func (export "qz_predict_v2") (param i64 i64 i32 i32 i32) (result f64)
 (local $i i32) (local $p f64) (local $f f64) (local $q f64) (local $v f64) (local $score f64)
 (loop $masks
  (if (i32.ne (i32.load (i32.add (i32.const 64) (i32.mul (local.get $i) (i32.const 4)))) (i32.const 0)) (then unreachable))
  (local.set $i (i32.add (local.get $i) (i32.const 1)))
  (br_if $masks (i32.lt_u (local.get $i) (i32.const 8))))
 (local.set $p (f64.load (i32.const 0)))
 (local.set $f (f64.load (i32.const 16)))
 (local.set $q (f64.load (i32.const 32)))
 (local.set $v (f64.load (i32.const 56)))
 (call $positive_finite (local.get $p))
 (call $positive_finite (local.get $f))
 (call $positive_finite (local.get $q))
 (if (i32.eqz (f64.lt (f64.abs (local.get $v)) (f64.const inf))) (then unreachable))
 (if (f64.gt (f64.abs (local.get $v)) (local.get $q)) (then unreachable))
 (local.set $score (f64.mul
  (f64.neg (f64.div (f64.sub (local.get $p) (local.get $f)) (local.get $f)))
  (f64.sub (f64.const 1) (f64.div (f64.abs (local.get $v)) (local.get $q)))))
 (if (i32.eqz (f64.lt (f64.abs (local.get $score)) (f64.const inf))) (then unreachable))
 (local.get $score))
)
'''
(out / 'B01.wat').write_text(wat)
(out / 'models.json').write_text(json.dumps({'B01': wat}, indent=2) + '\n')
previous = json.loads((out.parent / 'planned-liquidity-probe/research-request.json').read_text())
question = '''在已登记的523个Hyperliquid现货及永续标识和既定443日输入中，沿用原训练/验证/最终独立分区与60秒事件时间目标，检验一个因果单因子：以已完成成交簇VWAP p、同期快EMA f、总量q和方向净量v，令c=−(p−f)/f、b=v/q，分数为c×(1−|b|)。p、f、q须正且有限，v有限且|v|≤q，八槽任何不可用mask均拒绝，非有限输出失败；真实v=0允许，真实|v|=q产生策略零分。保持原共同EMA预热、全资产及成本假设，不叠加L01活跃条件，不扫窗口、参数、阈值或方向，不删除资产/日期；无成交期间没有新的模型调用。仅用训练/验证反馈，保留全范围覆盖、缺失原因、各状态指标和实际标签跨度；不查看最终独立结果选择参数，不伪称条件效应已由单因子IC证明。网络抖动0.1–1.5ms为假设，事件时间及历史数据PIT未证实，60秒标签不代表可按60秒成交。原目标为几千USDT账户牛熊震荡费用后夏普>5且年化>150%；IC不能证明可成交或收益。只计划一次试验，失败计入现有共享100次预算，原记录16/100须在提交前再次核对；保留前序失败与无净值结果。此文件是未提交草稿，不得把它当成运行授权；必要的拟议模型如下：\n\nB01 model:\n''' + wat
request = {
    'requestId': 'hyperliquid-flow-balance-B01-draft-20261030',
    'goal': question,
    'model': previous['model'],
    'thinkingLevel': previous['thinkingLevel'],
    'datasetId': previous['datasetId'],
    'maxTrials': 1,
    'publicLiteratureQueries': ['cryptocurrency market microstructure trade order flow price reversal'],
    'specification': {
        'version': 1,
        'name': 'Hyperliquid 成交方向平衡与C03回补 B01',
        'question': question,
        'rationale': '原始目标（摘要）：几千USDT资金下研究Hyperliquid牛熊震荡费用后夏普>5、年化>150%的因子、回测和组合，网络抖动0.1–1.5ms按回测假设。来源：用户请求与research-design.md。决策与交付物：本轮仅检验固定方向平衡加权C03的预测统计，并保留完整账本；经济目标仍须后续共享账户检验。Agent操作化：使用已完成簇的p、fastEMA、q、v，分数c×(1−|v/q|)，不声称盘口OFI或识别交易者。已知依据：C03验证pooled IC约0.27253但逐市场差异明显、经济净值严重缺失；L01验证IC约0.04161且小资金账户仍有残值。Bieganowski和Ślepaczuk论文摘要https://arxiv.org/abs/2602.00776使用Binance 1秒盘口与成交，报告短期特征关系及模拟，不证明本Hypothesis、Hyperliquid或盈利。机制猜想：强单向成交可能与偏离后的延续相伴；替代解释包括买卖价跳动、信息交易、时间错配和pooled偏权。单一Pearson IC不能证明相对C03的条件增益或分组斜率；需单独确认成对分层能力和覆盖。PIT、真实盘口、费用、资金费与FX和残值均有缺口。已有16/100次含失败，预算不能重置。',
        'exclusions': '不使用最终独立结果调参、不扫参数、不筛资产或日期、不将成交流称为盘口OFI、不以IC声称费用后收益或实际成交；本草稿不提交因子、不做账户绑定。',
        'evaluation': previous['specification']['evaluation'],
        'search': previous['specification']['search'],
    },
}
(out / 'research-request.draft.json').write_text(json.dumps(request, ensure_ascii=False, indent=2) + '\n')
