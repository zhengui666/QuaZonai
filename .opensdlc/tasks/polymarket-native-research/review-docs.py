from pathlib import Path


def replace(path, old, new):
    p = Path(path)
    s = p.read_text()
    assert s.count(old) == 1, (path, old)
    p.write_text(s.replace(old, new))


replace('DESIGN.md',
    '''Returns Volatility (252 days)/nautilus-analysis.ReturnsVolatility，
unit=ANNUALIZED_RETURN_STDDEV，annualization_factor=252；PORTFOLIO_SHARPE_RATIO对应
Sharpe Ratio (252 days)/nautilus-analysis.SharpeRatio，unit=RATIO，
annualization_factor=252。三者method_version=0.63.0、frequency=UTC_DAY；
252表示原生每年日数，不是再次乘到原值的系数。波动率/Sharpe由原生按UTC日''',
    '''Returns Volatility ({period} days)/nautilus-analysis.ReturnsVolatility，
unit=ANNUALIZED_RETURN_STDDEV，annualization_factor=period；PORTFOLIO_SHARPE_RATIO对应
Sharpe Ratio ({period} days)/nautilus-analysis.SharpeRatio，unit=RATIO，
annualization_factor=period。NAUTILUS_POLYMARKET 的 period=365，原股票／外汇路径
period=252；方法键必须与该请求实际选择的原生配置一致，不跨分支替换。三者
method_version=0.63.0、frequency=UTC_DAY；period 表示原生每年日数，不是再次乘到
原值的系数。日均收益不年化。波动率/Sharpe由原生按UTC日''')
replace('DESIGN.md',
    '## Polymarket 研究抵押币',
    '''### 冻结结算来源与目标期限

结算不是登记 BAR 后隐式授予的额外目录权限。原登记元数据的
quality.datasets[].settlements 冻结完整 NativeSettlementGroupV1：condition_id、
source_reference、恰好两个 outcomes；每项保留 instrument_id、close_price、
ts_event 和 ts_init。价格使用 DecimalValue，纳秒使用 DbCounter。不同 token
必须属于同一 condition，价格均在 [0,1] 并精确合计 1。单 token 研究也保留完整
来源向量，但不会给未选 token 建仓。不完整档案只能作为未验证数据准备，不能
通过缺项、重复 token 或零填充拼出完整结算。Sealed 质量元数据不公开兑付明细。

NativeDatasetSelectionV1、NativeSimulationRequestV1 和 NativePortfolioStudyRequestV1
分别冻结该 settlements；缺省空数组表示没有结算依据，不表示免费、零赔付或
可跳过结算。Store 从原登记证据生成请求，Runtime 对比同目录版本的完整可见
向量，Job 再核对原生 InstrumentClose 的价格、资产及两种时间；新增、缺失、
变更和重复记录均失败。未进入持有窗口或实际尚不可用的事件不会提前释放现金。
fresh DATA_VALIDATE 和组合质量产物保留原绑定，发布时复验。结算记录不作为
Alpha BAR 特征，也不传入其八参数 Wasm ABI。无需新鉴权服务、回测内核或账本。

所有二元目标要求 activation <= decision < expiration，且原 decision + TTL
不超过每个入选合约的 expiration。超期直接拒绝，不静默裁剪策略 TTL。原生
portfolio preparation、Runtime/Store 构建准入和 Candidate 发布使用同一规则；
Release 沿用既有原来源复验。交易目标到期不等于持仓已经结算：已建仓可继续
等待冻结来源的实际结算时点，但不能因此延长新交易权限。

## Polymarket 研究抵押币''')

p = Path('docs/polymarket-history.md')
s = p.read_text()
s += '''
## 结算登记字段与回放核对

数据版本的原生质量记录可包含 `settlements`。它是完整条件来源，不是预测特征：

| 字段 | 合同 |
| --- | --- |
| condition_id | 原条件 ID；同版本内唯一 |
| source_reference | 原始结算资料引用，1–2000 字节 |
| outcomes | 恰好两个不同 outcome token 的记录，不能重复或缺省另一边 |
| outcomes[].instrument_id | 原 `{condition_id}-{token_id}.POLYMARKET` |
| outcomes[].close_price | 原兑付比例，十进制字符串；各在 [0,1] 且精确合计 1 |
| outcomes[].ts_event | 原生事件纳秒，十进制字符串 |
| outcomes[].ts_init | 原实际可用纳秒，十进制字符串，不早于事件时间 |

把目录中的相应 InstrumentClose 纳入同一原生快照和质量元数据后，再沿用原
DataSource/数据版本登记和冻结输入流程。不能只往已经登记的 BAR 目录追加
close 文件并继续使用旧版本；需要新的不可变数据版本。修改原向量、文件中
价格、时间或 token，或缺失/重复其中记录，都会拒绝回放而非生成另一条净值。
同一接收时间的逐笔/盘口记录保留原输入顺序，不以事件时间反向重排。

Store 将登记中的完整可见向量绑定到科学任务；Runtime 复验元数据，Job 用
原生目录逐条核对，质量输出也保留原绑定。没有完整可见向量时不把某个价格
假定为结算。只交易一侧时仍核对另一侧来源，但不为其建立头寸或分配资金。
Alpha 的 BAR/八参数 Wasm 特征不读取这些终局明细。

目标有效期和资金结算时间分别处理：构建的原 TTL 必须完整落在所有入选
BinaryOption 的有效期内；越过到期、已经到期或尚未激活的目标直接拒绝。
到期后仍可等待实际结算事件回放原持仓，这不恢复下单权限，也不声称已经
代办链上赎回。合并/执行状态以 PR 的最终 CI 和审查记录为准。
'''
p.write_text(s)

p = Path('.opensdlc/tasks/polymarket-native-research/task.md')
s = p.read_text()
s += '''
## First review corrections

The first independent review identified unbound close rows, target TTL beyond binary
expiry, incoherent sibling payouts and inconsistent annualization documentation.
Corrections freeze complete source payout vectors in registered quality metadata and
native task contracts, recheck native close records before replay, and enforce the
original contract lifetime at build/adoption boundaries. No source scan grants its own
scientific authority. The importer also preserves original arrival order for equal
reception times. Native, domain and real-journal regressions cover these cases.

These changes require final-head CI and another explicit read-only clean review in
PR #101; their authorship alone is not a successful executor or acceptance result.
The detailed remediation specification is recorded in Issue #100 comment 5770031108.
'''
p.write_text(s)
print('Canonical detailed metrics and settlement/target contracts synchronized.')
