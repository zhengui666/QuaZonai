/** Display-only definitions. Values, units and scientific identities remain server-owned. */
export interface MetricDefinition { label: string; explanation: string }
const definition = (label: string, explanation: string): MetricDefinition => ({ label, explanation });
const nativeMoney = '单位为原生报告币种；接口未提供币种时不推断。历史结果不代表未来收益。';
const fraction = '按原生小数比例显示，0.01 表示 1%；不在浏览器重新年化。';
const glossary: Record<string, MetricDefinition> = {
  pearson_ic: definition('皮尔逊信息系数（IC）', '因子分数与对应未来标签的皮尔逊相关系数，无量纲，范围为 −1 至 1。不是收益率；恒定分数、恒定标签或样本不足时不可用。'),
  'counts.observations': definition('总观测', '本分区收到的原始观测数，单位：条。包含后来被排除的观测，不等于独立交易次数。'),
  'counts.included': definition('有效观测', '通过原生对齐和完整性检查、实际用于该分区指标的观测数，单位：条。不据此假定样本彼此独立。'),
  'counts.excluded': definition('排除观测', '未参与该分区指标计算的观测数，单位：条。具体原因见预热、标签与分区计数；缺失数据不补零。'),
  'counts.indicator_warmup': definition('预热观测', '因指标历史长度尚未满足而排除的观测数，单位：条。'),
  'counts.incomplete_labels': definition('标签不完整', '因未来标签所需数据不完整而排除的观测数，单位：条。'),
  'counts.cross_boundary_labels': definition('跨区间标签', '因标签窗口越过原登记分区边界而排除的观测数，单位：条。'),
  'counts.prediction_outside_partition': definition('区间外预测', '预测时钟不属于当前分区的观测数，单位：条。不混入当前分区指标。'),
  'PnL (total)': definition('总盈亏', `原生账户报告的总盈亏，包含该报告纳入的未实现盈亏。${nativeMoney}`),
  'PnL% (total)': definition('总盈亏百分比', '原生账户总盈亏相对初始余额的百分比，单位：%。保留原值，不等于年化收益。'),
  'Max Winner': definition('最大单笔盈利', `已实现盈利记录中的最大金额。${nativeMoney}`),
  'Avg Winner': definition('平均单笔盈利', `已实现盈利记录的平均金额，不含亏损记录。${nativeMoney}`),
  'Min Winner': definition('最小单笔盈利', `已实现盈利记录中的最小金额。${nativeMoney}`),
  'Max Loser': definition('最大单笔亏损', `已实现亏损记录中损失最大的金额，保留原生负号。${nativeMoney}`),
  'Avg Loser': definition('平均单笔亏损', `已实现亏损记录的平均金额，保留原生负号。${nativeMoney}`),
  'Min Loser': definition('最小单笔亏损', `已实现亏损记录中最接近零的金额，保留原生负号。${nativeMoney}`),
  Expectancy: definition('单笔期望盈亏', `原生平均盈利 × 盈利占比，加平均亏损 × 亏损占比；不是未来预期回报。${nativeMoney}`),
  'Win Rate': definition('胜率', `原生已实现盈利记录占有效盈亏记录的比例；不衡量盈亏幅度。${fraction}`),
  'Average (Return)': definition('平均收益', `原生账户收益序列的算术平均值，不是复合收益。${fraction}`),
  'Average Win (Return)': definition('平均正收益', `原生收益序列中正收益的算术平均值。${fraction}`),
  'Average Loss (Return)': definition('平均负收益', `原生收益序列中负收益的算术平均值，保留负号。${fraction}`),
  'Returns Skewness': definition('收益偏度', '原生收益分布的不对称程度，无量纲。样本不足或波动为零时可能不可用。'),
  'Returns Kurtosis': definition('收益峰度', '原生收益分布相对正态分布的超额峰度，无量纲。对极端样本敏感，不是损失概率。'),
  'Profit Factor': definition('盈利因子', '原生正收益之和除以负收益之和的绝对值，无量纲。无负收益时可能不可用；不代表因子的因果有效性。'),
  'Risk Return Ratio': definition('收益风险比', '原生日收益均值除以标准差，无量纲、未年化。不是夏普比率的额外年化版本。'),
  'Long Ratio': definition('多头持仓占比', `原生持仓记录中以买入开仓的记录占比，不是资金权重。${fraction}`),
  'Tail Ratio': definition('尾部收益比', '原生收益分布第 95 百分位与第 5 百分位绝对值之比，无量纲。样本很少或分母为零时不可用。'),
  'Max Drawdown': definition('原生最大回撤', `原生收益复合曲线相对此前峰值的最深跌幅，负值表示回撤。${fraction}与下方权益曲线的显示投影口径分开保留。`),
  'Calendar Expected Days': definition('日历应覆盖天数', '原生登记区间应包含的完整 UTC 日数，单位：天。区间边界不完整时不可用。'),
  'Calendar Observed Return Days': definition('日历已观测收益天数', '原生账户实际提供日收益记录的天数，单位：天。不会补齐缺日。'),
  'Calendar Coverage Ratio': definition('日历收益覆盖率', `已观测收益天数除以应覆盖天数；仅表示记录覆盖。${fraction}`),
  'Calendar Geometric Daily Return': definition('日历几何日均收益', `完整原生逐日收益的几何平均值，不是年化收益。${fraction}覆盖不完整时不可用。`),
  'Calendar Minimum Daily Return': definition('日历最低日收益', `完整原生日收益序列中的最小值。${fraction}`),
  'Calendar Maximum Daily Return': definition('日历最高日收益', `完整原生日收益序列中的最大值。${fraction}`),
  'Calendar Maximum Absolute Deviation': definition('日历最大绝对偏差', `日收益与几何日均收益之间的最大绝对差。${fraction}不是置信区间。`),
  'Calendar Negative Days': definition('日历亏损天数', '完整日历收益中小于零的天数，单位：天。缺日时不可用，不将缺日视为零收益。'),
  'Calendar Zero Days': definition('日历零收益天数', '完整日历收益中恰为零的天数，单位：天。不包含缺失日。'),
  capital: definition('本金', '为本次原生共享账户回测明确提供的初始资金，单位：报告币种。保留精确金额，不加总成员各自的独立本金。'),
  weight: definition('权重', '原政策中的资金或目标敞口比例，无量纲，1 表示 100%。保持原始精度；因子分数不自动当作权重。'),
  equity: definition('账户权益', '权威账户快照或原生共享账户的总权益，单位：账户币种。可包含未实现盈亏；不是可提取现金。缺失时保持未知。'),
  cash: definition('现金', '权威账户快照提供的现金金额，单位：账户币种。并非全部可交易或可提取，仍需扣除原生预留及其他约束。'),
  endingEquity: definition('期末权益', '全区间最后一条原生账户权益，单位：报告币种。未补点、未四舍五入；时间或末值缺失时不可用。'),
  cumulativeReturn: definition('累计收益', '权益相对本次初始本金的变化百分比，单位：%。仅为原生权益的显示投影，未年化、未调整外部资金流，不代替原生收益统计。'),
  drawdown: definition('回撤', '权益相对此前最高权益（含初始本金）的变化百分比，单位：%，负值表示下降。缺失权益或无效时间时不补点计算。'),
  maxDrawdown: definition('最大回撤', '全区间权益相对此前峰值的最深跌幅，单位：%，负值表示回撤。来自原生权益的显示投影；区间选择不会改变全区间摘要。'),
  records: definition('记录点', '原生账户中时间可解析的权益记录数量，单位：条。同一毫秒内的原始时钟仍逐条保留；不是独立样本数。'),
  quantity: definition('数量', '权威账户给出的持仓数量，单位由原标的定义决定。保持原精度，不根据标的名称猜测合约乘数。'),
  maximumReductionQuantity: definition('最多数量', '原生受限减仓计划允许减少的最大数量，单位由原标的定义决定。不是必须成交的数量。'),
  minimumSellPrice: definition('最低卖价', '本次受限减仓允许的最低卖出价格，单位由原标的价格币种决定。不是当前市价或成交保证。'),
  cancellationsAndLegs: definition('取消开启订单 / 减仓腿', '原生计划拟撤销的增加敞口订单数与显式减仓腿数，单位：个。不代表撤销或减仓已经完成。'),
  positionValue: definition('市值', '权威账户给出的该持仓估值，单位：账户币种。估值未提供时不以数量或最近价格补算。'),
  pnl: definition('盈亏', '权威账户给出的该持仓盈亏，单位：账户币种。保留负号与原值，不推断未提供的盈亏。'),
  reserved: definition('账户总预留', '原生所有者报告的账户级预留总额，单位：账户币种。暂停、取消并不自动释放；未知时不能按零计算。'),
  exitReserved: definition('保留预留', '原生所有者为本资金计划保留的金额，单位：账户币种。不是账户总预留；暂停或取消计划不代表预留已释放。'),
  remainingManagedCapital: definition('剩余可交易资金', '本次资金退出预览在原生资金与执行政策约束下报告的剩余管理资金，单位：账户币种。不是单个策略的建议资金，也不代表提取已完成。'),
  cashBuffer: definition('显式现金缓冲', '用户明确留作现金、不分配给策略的资金，单位：账户币种。在账户级预留之外扣除。'),
  budget: definition('扣预留后的可用权益', '账户权益扣除账户总预留和显式现金缓冲后的资金预算，单位：账户币种。不是已成交资金或保证可提取额。'),
  strategyCapital: definition('有效策略资金', '在预算、联合容量、流动性、费用和订单增量约束下分配的资金，单位：账户币种。建议目标不代表实际成交。'),
  residualCash: definition('现金 / 费用后剩余', '本次资金计算完成后未分配的剩余资金，单位：账户币种。按原生结果保留，不把它等同于可提取余额。'),
  scale: definition('预算缩放', '本次成员建议资金相对原资金基准的缩放系数，无量纲。只缩放原选池，不重新选择因子。'),
  requestedAmount: definition('请求金额', '明确请求预留并在外部提取的金额，单位：账户币种。请求不代表原生预留已确认，更不代表转账已完成。'),
  releasedCash: definition('已释放现金', '原生所有者报告已从持仓释放的现金，单位：账户币种。仍需可提取证据和外部对账。'),
  withdrawable: definition('有证据的可提取额', '原生所有者根据可提取证据确认的金额，单位：账户币种。模拟证据不证明实盘可提取；未知不补零。'),
  reconciledWithdrawal: definition('已对账外部提取', '下游已核对实际外部现金变动的累计金额，单位：账户币种。QZ 本身不转账。'),
  executionCost: definition('预计执行成本', '原生计划根据其成本证据给出的估计金额，单位：账户币种。不是既有未实现盈亏，也不是最终实际费用。'),
  unrealizedPnl: definition('既有未实现盈亏（非提取成本）', '原生计划单独报告的既有持仓未实现盈亏，单位：账户币种。不是本次资金提取引起的执行成本。'),
  economicCapacity: definition('经济容量', '在匹配资金规模的样本外净收益、市场校准冲击、价差费用、执行时序与退出证据下才能评估的盈利容量，单位：报告币种。历史 BAR 参与率上限不能替代它；缺少输入时保持不可用。'),
  decisionNs: definition('决策时钟', '原生限制决策的 Unix 纳秒时钟。可读时间按 UTC 显示，下方保留完整纳秒整数，不将整个时钟转换为 JavaScript 浮点数。'),
  barEventNs: definition('BAR 事件时钟', '约束所用原始 BAR 的事件时间。按 UTC 与完整 Unix 纳秒同时保留；不是数据已经可用于决策的时刻。'),
  barAvailableNs: definition('BAR 可得时钟', '原始 BAR 在系统中可用于决策的时刻。必须不晚于该决策时钟；不以抓取时间或未来数据补造可得性。'),
  maximumCapital: definition('历史可执行上限', '在原执行映射、成交量参与率、建仓／再平衡和独立全量退出假设下，历史 BAR 约束给出的最紧资金上限，单位：报告币种。不是盈利最优本金、未来深度或收益承诺。'),
  minimumCapital: definition('最低实用本金', '在原生最小订单、数量与资金精度下得到的最低可用资金，单位：报告币种。低于此值可能无法构建有效订单。'),
  participation: definition('成交量参与率', '明确假设允许策略占历史 BAR 名义成交量的最大比例，无量纲，0.01 表示 1%。不是已测量的真实成交概率。'),
  maximumAgeSeconds: definition('流动性数据最大时差', '决策时刻与所用可得 BAR 之间允许的最大时差，单位：秒。超出或缺失时不使用更晚数据补齐。'),
  scenarioCapital: definition('资本情景', '用于原生容量计算的精确试算资金，单位：报告币种。不是已投资资金；情景之间不是概率分布。'),
  requiredTurnover: definition('峰值 BAR 需求', '情景资金在单标的／单 BAR 上的峰值名义需求，单位：报告币种。取累计建仓再平衡与独立全量退出需求的较大者，不是单笔订单或全期累计成交额。'),
  maximumParticipation: definition('最大参与率', '情景中最紧标的／BAR 的所需名义量占可观测成交量的比例，无量纲。0.01 表示 1%；不是市场冲击估计。'),
  usableCapital: definition('可用本金', '本情景在历史容量与原生最小订单约束下可分配的本金，单位：报告币种。不是盈利容量，也不是下游已用资金。'),
  unallocatedCash: definition('剩余现金', '本资本情景扣除可分配本金后的未分配资金，单位：报告币种。保留原生十进制结果。'),
  volumeMultiplier: definition('成交量倍数', '对原观测 BAR 成交量施加的明确假设倍数，无量纲。1 为基准，0.5 为假设减半；不是预测或置信水平。'),
  entryRebalanceTurnover: definition('建仓／再平衡需求', '同一标的／BAR 预算累计承接的绝对净目标变化对应名义量，单位：报告币种。重复使用同一 BAR 不会补充流动性。'),
  exitTurnover: definition('全量退出需求', '独立全量退出试算所需的峰值名义量，单位：报告币种。作为替代清算约束，不假定与策略换仓同时发生。'),
  entryRebalanceParticipation: definition('建仓／再平衡参与率', '建仓与再平衡名义需求占其 BAR 观测名义量的比例，无量纲。0.01 表示 1%。'),
  exitParticipation: definition('退出参与率', '独立全量退出名义需求占其 BAR 观测名义量的比例，无量纲。不是退出一定成功的概率。'),
  decisionCount: definition('决策数', '容量请求包含的原始决策数，单位：次。不等于完成评估的决策数。'),
  assessedDecisionCount: definition('已评估决策数', '容量计算实际完成的原始决策数，单位：次。失败报告可只覆盖部分决策。'),
  requiredObservations: definition('所需观测数', '存在需求的标的／决策所需流动性观测总数，单位：条。缺失项不补齐。'),
  coveredObservations: definition('已覆盖观测数', '实际取得合格 BAR 的标的／决策观测数，单位：条。与所需观测一起判断覆盖。'),
  uniqueBars: definition('独立 BAR 数', '按原标的、事件与可得时钟区分的 BAR 数，单位：根。同一根 BAR 的流动性预算不会因重复使用而重置。'),
  reusedBarObservations: definition('重复使用 BAR 次数', '后续决策再次引用既有 BAR 的观测次数，单位：次。对应需求累计消耗同一预算。'),
  observedNotional: definition('观测名义成交量', '限制条件所用历史 BAR 的名义成交量，单位：报告币种。不代表当时盘口深度或未来可成交量。'),
  requiredWeight: definition('约束需求权重', '该 BAR 约束所需的累计绝对净目标变化或退出权重，无量纲。用于原生资金上限计算，不是因子分数。'),
  inputTokens: definition('输入词元', '官方模型用量报告的输入 Token 数，单位：词元。未报告不记为零，可能包含提供者定义的缓存用量。'),
  outputTokens: definition('输出词元', '官方模型用量报告的输出 Token 数，单位：词元。未报告不记为零。'),
  cost: definition('模型费用', '官方提供者用量报告记录的费用金额。按报告原值显示；未提供币种或费用时不推算。'),
  activeTasks: definition('活动任务', '官方 Durable 当前记录的活动任务数量，单位：个。不同于研究数或正在运行的科学进程数。'),
  trials: definition('试验次数', '原研究账本已经记录的试验次数，单位：次。失败、拒绝及取消仍占用原预算，不是成功结果数。若显示 a / b，b 为原始试验预算。'),
  members: definition('成员数', '组合绑定的原子因子或历史研究成员数量，单位：个。成员共享一次原生账户模拟，不加总各自权益。'),
};
const annualized: Record<string, [string, string]> = {
  'Sharpe Ratio': ['夏普比率', '原生日收益均值除以标准差，再乘指定年化周期的平方根；原生假设无风险利率为零。无量纲，零波动或样本不足时不可用。'],
  'Sortino Ratio': ['索提诺比率', '原生日收益均值相对下行波动的比率，使用指定年化周期。无量纲，下行波动为零或样本不足时不可用。'],
  'Returns Volatility': ['收益波动率', `原生日收益标准差按指定周期年化。${fraction}`],
  'Information Ratio': ['信息比率', '原生相对基准的平均主动收益除以跟踪误差，按指定周期年化，无量纲。需要原生匹配基准，不在前端补算。'],
  'Tracking Error': ['跟踪误差', `原生相对基准的主动收益标准差，按指定周期年化。${fraction}`],
  Alpha: ['阿尔法', '原生相对基准回归得到的超额收益参数，按原生指定周期报告。需要匹配基准，不在前端构建基准。'],
  'Treynor Ratio': ['特雷诺比率', '原生收益相对系统风险贝塔的比率，按指定周期报告。无量纲，贝塔为零或基准不足时不可用。'],
  'Up Capture Ratio': ['上涨捕获比', '原生基准上涨期间策略与基准收益之比，按指定周期报告。保持原生比例单位，不自行转换。'],
  'Down Capture Ratio': ['下跌捕获比', '原生基准下跌期间策略与基准收益之比，按指定周期报告。保持原生比例单位，不自行转换。'],
};
const unknown = definition('未识别指标', '服务返回了尚未收录的指标。数值与原始单位保持不变，定义暂未确认；请在“原始字段”中核对技术名称，不据此推断收益或单位。');
export function metricDefinition(key: string): MetricDefinition {
  const normalized = key.replace(/^(?:TRAIN|VALIDATION|HELDOUT|DISCOVERY|SEALED|FORWARD)\./, '').replace(/^portfolio\./, '');
  if (normalized === 'ic') return glossary.pearson_ic!;
  if (glossary[normalized]) return glossary[normalized]!;
  const period = /^(.*?) \((\d+) days\)$/.exec(normalized);
  if (period && annualized[period[1]!]) {
    const [label, explanation] = annualized[period[1]!]!;
    return definition(`${label}（${period[2]} 天年化）`, `${explanation}年化周期为原生字段指定的 ${period[2]} 天；不按市场名称改写。`);
  }
  const extra: Record<string, MetricDefinition> = {
    Beta: definition('贝塔', '原生策略收益相对基准收益的回归敏感度，无量纲。没有匹配基准时不可用。'),
    'Ulcer Index': definition('溃疡指数', '原生收益曲线回撤平方的均值再开方，用于描述持续下行程度。按原生数值口径保留，不当作损失概率。'),
  };
  if (extra[normalized]) return extra[normalized]!;
  const threshold = /^Omega Ratio \(threshold ([-\d.]+)\)$/.exec(normalized);
  if (threshold) return definition(`欧米伽比率（阈值 ${threshold[1]}）`, '收益超过与低于原生指定阈值的幅度之比，无量纲。不在浏览器改变阈值。');
  const risk = /^(Value at Risk|Expected Shortfall) \(confidence ([\d.]+)\)$/.exec(normalized);
  if (risk) return definition(`${risk[1] === 'Value at Risk' ? '风险价值' : '预期尾部损失'}（置信水平 ${risk[2]}）`, '原生历史收益分布在指定置信水平下的尾部风险统计，保留原生收益单位。不是容量置信区间或最坏损失保证。');
  return unknown;
}

/** Existing facts use Chinese labels; aliases keep their explanations in the same glossary. */
const aliases: Record<string, string> = {
  '总权益': 'equity', '权益': 'equity', '账户权益': 'equity', '现金': 'cash', '本金': 'capital', '权重': 'weight', '目标权重': 'weight', '成员': 'members',
  '上限': 'maximumCapital', '历史可执行上限': 'maximumCapital', '条件容量上限': 'maximumCapital', '最低实用本金': 'minimumCapital', '约束资金上限': 'maximumCapital',
  '所用成交量参与率': 'participation', '成交量参与率': 'participation', '流动性数据最大时差': 'maximumAgeSeconds', '资本情景': 'scenarioCapital',
  '账户总预留': 'reserved', '保留预留': 'exitReserved', '显式现金缓冲': 'cashBuffer', '扣预留后的可用权益': 'budget', '有效策略资金': 'strategyCapital', '交易预算': 'strategyCapital', '本次建议资金': 'strategyCapital', '剩余可交易资金': 'remainingManagedCapital',
  '现金 / 费用后剩余': 'residualCash', '预算缩放': 'scale', '请求金额': 'requestedAmount', '已释放现金': 'releasedCash', '有证据的可提取额': 'withdrawable', '已对账外部提取': 'reconciledWithdrawal', '预计执行成本': 'executionCost', '既有未实现盈亏（非提取成本）': 'unrealizedPnl',
  '取消开启订单 / 减仓腿': 'cancellationsAndLegs', '输入 Token': 'inputTokens', '输出 Token': 'outputTokens', '费用': 'cost', '活动任务': 'activeTasks',
};
export function factMetricKey(label: string): string | undefined { return aliases[label]; }
export function partitionLabel(value: string): string { return ({ TRAIN: '训练', VALIDATION: '验证', HELDOUT: '独立样本', DISCOVERY: '发现', SEALED: '封存', FORWARD: '前向' } as Record<string, string>)[value] ?? '未识别分区'; }
