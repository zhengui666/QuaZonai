import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { CapitalExitFacts, CapitalExitPreviewFacts } from './capital-exits';
import { exit, preview, time, until, pausePendingExits, pendingCancelExit, id, source, intent } from './capital-exit-fixtures';

describe('actual capital exit readout components', () => {
  it('renders all monetary facts without fabricated zeros or real simulated withdrawability', () => {
    const html = renderToStaticMarkup(<CapitalExitFacts exit={exit} now={Date.parse(time)} />);
    for (const text of ['模拟可用现金', '申请退出', '禁止再投入', '已释放现金', '尚未释放', '暂无证据', '9007199254740993.123456789 USD', '等待合格兼容目标', '原始计划证据', '最近记录阶段']) expect(html).toContain(text);
    expect(html).not.toContain('可在交易所提款'); expect(html).not.toContain('已核验可提金额');
    expect(html).not.toContain('withdrawal-address');
  });
  it('shows cancelled reserve and historical partial release instead of claiming reversal', () => {
    const html = renderToStaticMarkup(<CapitalExitFacts exit={{ ...exit, state: 'CANCELLED_RESERVED' }} now={Date.parse(until)} />);
    expect(html).toContain('后续退出已取消，资金仍保留'); expect(html).toContain('10 USD'); expect(html).toContain('暂无有效模拟证据');
  });
  it('renders exact immutable scope, unknown risk/cost and blocked reason', () => {
    const html = renderToStaticMarkup(<CapitalExitPreviewFacts preview={{ ...preview, environment: 'LIVE', capability: 'BLOCKED', reason_codes: ['account_owner_binding_unavailable'] }} now={Date.parse(time)} />);
    for (const text of ['LIVE', 'BLOCKED', 'account_owner_binding_unavailable', '暂无证据', '已有未实现盈亏', '预计增量执行成本', '仍需减仓释放', '原生锁定现金', '精确执行约束', 'CASH_ONLY', '保留保护订单']) expect(html).toContain(text);
    expect(html).not.toContain('开始退出');
  });
  it('keeps pause/cancel acceptance and FenceApplied distinct from completion in facts and timeline', () => {
    for (const record of [...pausePendingExits, exit, pendingCancelExit]) {
      const html = renderToStaticMarkup(<CapitalExitFacts exit={record} now={Date.parse(time)} />);
      expect(html).not.toContain('已暂停'); expect(html).not.toContain('后续退出已取消');
      expect(html).toContain('最近记录阶段：限价减仓阶段'); expect(html).not.toContain('最近记录阶段：正在');
      for (const reason of record.reason_codes) expect(html).toContain(reason);
    }
  });
  it('marks stale pause and cancel displays as previous records', () => {
    for (const record of [exit, pendingCancelExit]) {
      const html = renderToStaticMarkup(<CapitalExitFacts exit={record} now={Date.parse(time)} readCurrent={false} />);
      expect(html).toContain('上次记录：'); expect(html).toContain('上次记录状态');
    }
  });
  it('retains complete historical portfolio scope, multiple legs and all evidence references', () => {
    const record = { ...exit, scope: { kind: 'PORTFOLIO_SCOPE' as const, amount: exit.scope.amount, currency: 'USD', stream_ids: [id, source], release_ids: [intent, id] },
      policy: { kind: 'BOUNDED_LIMIT' as const, deadline: until, legs: [
        { instrument_id: 'BTC-USD.NATIVE', maximum_reduction_quantity: '9007199254740993.123456789', minimum_sell_price: '1.000000001' },
        { instrument_id: 'ETH-USD.NATIVE', maximum_reduction_quantity: '2.000000001', minimum_sell_price: '3.000000001' },
      ], max_execution_cost: { amount: '0.000000001', currency: 'USD', reference_evidence_id: source } }, evidence_refs: [id, source, intent] };
    const original = structuredClone(record);
    const html = renderToStaticMarkup(<CapitalExitFacts exit={record} now={Date.parse(time)} />);
    for (const text of ['原始退出范围（只读）', '原始执行约束（只读）', 'stream_ids', 'release_ids', id, source, intent,
      'BTC-USD.NATIVE', 'ETH-USD.NATIVE', '9007199254740993.123456789', '2.000000001', '0.000000001']) expect(html).toContain(text);
    expect(record).toEqual(original);
  });
});
