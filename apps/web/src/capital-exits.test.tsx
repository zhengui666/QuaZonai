import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { CapitalExitFacts, CapitalExitPreviewFacts } from './capital-exits';
import { exit, preview, time, until } from './capital-exit-fixtures';

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
});
