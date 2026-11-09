from pathlib import Path
import json

out = Path(__file__).parent
# Reuse L01's ABI setter without executing or changing its builder.
setter = (out.parent / 'planned-liquidity-probe/L01.wat').read_text().split('(func $positive_finite')[0]
wat = setter + '''(func $finite (param $v f64)
 (if (i32.eqz (f64.lt (f64.abs (local.get $v)) (f64.const inf))) (then unreachable)))
(func $positive_finite (param $v f64)
 (call $finite (local.get $v))
 (if (i32.eqz (f64.gt (local.get $v) (f64.const 0))) (then unreachable)))
(global $count (mut i32) (i32.const 0))
(global $next (mut i32) (i32.const 0))
(func (export "qz_predict_v2") (param i64 i64 i32 i32 i32) (result f64)
 (local $i i32) (local $at i32) (local $r f64)
 (local $dt f64) (local $q f64) (local $v f64) (local $sumr f64) (local $absr f64)
 (loop $masks
  (if (i32.ne (i32.load (i32.add (i32.const 64) (i32.mul (local.get $i) (i32.const 4)))) (i32.const 0)) (then unreachable))
  (local.set $i (i32.add (local.get $i) (i32.const 1)))
  (br_if $masks (i32.lt_u (local.get $i) (i32.const 8))))
 (call $positive_finite (f64.load (i32.const 0)))
 (call $positive_finite (f64.load (i32.const 8)))
 (call $positive_finite (f64.load (i32.const 32)))
 (call $positive_finite (f64.load (i32.const 48)))
 (call $finite (f64.load (i32.const 56)))
 (local.set $r (f64.sub (f64.div (f64.load (i32.const 0)) (f64.load (i32.const 8))) (f64.const 1)))
 (call $finite (local.get $r))
 (local.set $at (i32.add (i32.const 128) (i32.mul (global.get $next) (i32.const 40))))
 (f64.store (local.get $at) (f64.load (i32.const 48)))
 (f64.store offset=8 (local.get $at) (f64.load (i32.const 32)))
 (f64.store offset=16 (local.get $at) (f64.load (i32.const 56)))
 (f64.store offset=24 (local.get $at) (local.get $r))
 (f64.store offset=32 (local.get $at) (f64.abs (local.get $r)))
 (global.set $next (i32.and (i32.add (global.get $next) (i32.const 1)) (i32.const 31)))
 (if (i32.lt_u (global.get $count) (i32.const 32)) (then
  (global.set $count (i32.add (global.get $count) (i32.const 1)))))
 (if (i32.lt_u (global.get $count) (i32.const 32)) (then (return (f64.const 0))))
 ;; Recompute the short window so expired huge intervals leave no subtraction residue.
 (local.set $i (i32.const 0))
 (loop $sum_window
  (local.set $at (i32.add (i32.const 128) (i32.mul (local.get $i) (i32.const 40))))
  (local.set $dt (f64.add (local.get $dt) (f64.load (local.get $at))))
  (local.set $q (f64.add (local.get $q) (f64.load offset=8 (local.get $at))))
  (local.set $v (f64.add (local.get $v) (f64.load offset=16 (local.get $at))))
  (local.set $sumr (f64.add (local.get $sumr) (f64.load offset=24 (local.get $at))))
  (local.set $absr (f64.add (local.get $absr) (f64.load offset=32 (local.get $at))))
  (local.set $i (i32.add (local.get $i) (i32.const 1)))
  (br_if $sum_window (i32.lt_u (local.get $i) (i32.const 32))))
 (if (f64.gt (local.get $dt) (f64.const 60000000000)) (then (return (f64.const 0))))
 (call $positive_finite (local.get $q))
 (call $finite (local.get $v))
 (call $finite (local.get $sumr))
 (call $finite (local.get $absr))
 (if (f64.eq (local.get $absr) (f64.const 0)) (then (return (f64.neg (f64.div (local.get $v) (local.get $q))))))
 (f64.mul (f64.neg (f64.div (local.get $v) (local.get $q)))
  (f64.sub (f64.const 1) (f64.div (f64.abs (local.get $sumr)) (local.get $absr)))))
)
'''
(out / 'A01.wat').write_text(wat)
(out / 'models.json').write_text(json.dumps({'A01': wat}, indent=2) + '\n')
previous = json.loads((out.parent / 'planned-liquidity-probe/research-request.json').read_text())
goal = '''保持完整登记的523个Hyperliquid现货/永续标识、443日原始输入、既定训练/验证/最终独立分区和60秒事件时间目标，仅准备A01一次单因子试验：方向流与价格路径效率背离的吸收代理。每品种从共同8/32价格EMA预热后首次有效qz_predict_v2调用开始独立保存32个完整间隔及对应簇，不虚构预热前数据。对这32项令r_i=p_i/p_previous_i−1，B=Σsigned_quantity_i/Σquantity_i，E=abs(Σr_i)/Σabs(r_i)。注意Σr_i明确是逐簇简单收益的求和，不是起终价收益、连乘收益或log收益；不得替换公式。若Σabs(r_i)真实为0，定义E=0，分数为−B。仅在已有32个完整间隔且T32=Σelapsed_event_ns<=60,000,000,000纳秒时返回A01=−B·(1−E)，否则返回明确策略零分；恰好60秒准入，阈值与窗口固定，不扫参数、翻符号或事后筛选市场/日期。零分是策略选择，不是缺失价格填零。八个输入槽mask0才可用；任何mask1/2/3先失败且不修改窗口；实际使用的p、previous_p、quantity和dt须正且有限，signed_quantity须有限，计算的r须有限，均在状态更新前检查。signed_quantity的真实0或负值有效，不能当缺失。所有输出须有限。只用当时已完成簇与独立品种状态；不使用未来成交、未来退市、未来资金率、钱包/订单/TWAP身份或最终独立结果。机制猜想是单向主动成交未形成有效同向价格路径时可能被对手流动性吸收；代理不能识别真实冰山单、算法或TWAP，成交价跳动也可产生相同现象。只报告训练/验证预测统计、零分比例、全标识覆盖及实际标签跨度，不调用factor_finalize，不查看heldout调公式。历史事件时间proxy不证明真实接收时钟PIT。保留原失败4次、纠正4次、记忆4次及L01一次：累计13/100，剩87；草稿maxTrials=1只在root核对原账本并正式admit后消耗原余额，若实际提交累计最多14/100，失败也计数，不能免费重试或新开100次。前序研究23601、23690、23752、23821全部保留；L01因子验证IC约0.0416但费用后308天仅71有效收益日，验证三行情有效收益日均0，费用约1733.521 USDC，不构成净收益支持；C03经济结果同样不足。A01不是L01阈值修正，而是新方向流与价格路径互动机制。原目标仍是几千USDT、0.1–1.5ms网络抖动假设下，牛熊震荡费用后夏普>5、年化>150%；IC不是账户收益。后续经济检验保持原3000USDT、费用、延迟、参与率、最小订单及1小时估值期限；全未知日、dust和未成交保留，无成交时模型不调用，返回0不保证退出，分数归一化也不保证降换手。本草稿不授权提交或账户回放；root决定admission。不得更改登记输入或成本/估值政策，不编造价格。以下WAT是唯一预定模型，只允许一次factor_evaluate，保留失败并停下，不最终选择。\n\nA01 model:\n''' + wat
request = {
    'requestId': 'hyperliquid-absorption-A01-draft-20261029-05',
    'goal': goal, 'model': previous['model'], 'thinkingLevel': previous['thinkingLevel'],
    'datasetId': previous['datasetId'], 'maxTrials': 1,
    'publicLiteratureQueries': ['Algorithmic trading execution behavior and order splitting'],
    'specification': {
        'version': 1, 'name': 'Hyperliquid方向流—价格路径吸收代理 A01', 'question': goal,
        'rationale': '原始目标（摘要）：用户要求Hyperliquid几千USDT因子、回测和组合，探索算法行为与小资金套利，在牛熊震荡费用后夏普>5且年化>150%。来源：owner-intent.json、search-design.md、原研究23601/23690/23752/23821及保存的C03/L01主要训练验证经济结果。Agent操作化：保留13次前序与所有负面/未知结果，提出一个方向流与价格效率背离的全范围代理，最多一次新尝试，未提交。文献仅核对摘要：Cont/Kukanov/Stoikov arXiv:1011.6402研究50只美国股票，发现盘口OFI关系较成交量关系稳健；本输入是成交方向流，不是盘口OFI，不能借此声称同样稳健。Bonart/Gould arXiv:1511.04116研究Nasdaq市价单前后订单流，观察不同阶段，部分结果符合stimulated refill，另一些不符合；没有证明本A01、加密市场或盈利。只用作机制背景与反证提醒，未阅读全文。实际机制仍属猜想：可能吸收，也可能买卖价跳动、tick离散和混合流；若独立于原C03的验证作用消失、费用吞噬或净值不可算，则不支持经济目标。来源链接https://arxiv.org/abs/1011.6402 与 https://arxiv.org/abs/1511.04116。',
        'exclusions': '不称真实算法/TWAP/冰山单识别，不称套利；不筛资产或日期、不扫参数、不翻符号、不补价格、不改估值期限、不清除dust、不最终选择、不以IC代净收益。',
        'evaluation': previous['specification']['evaluation'], 'search': previous['specification']['search'],
    },
}
(out / 'research-request.draft.json').write_text(json.dumps(request, ensure_ascii=False, indent=2) + '\n')
