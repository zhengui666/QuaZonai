import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { NativeInputFacts } from './native-input';

describe('native input evidence presentation', () => {
  it('does not relabel a loaded tick archive as BAR or scientific qualification before a study contract exists', () => {
    const html = renderToStaticMarkup(createElement(NativeInputFacts, { modality: 'TRADE_TICK', preparation: { archiveLoaded: true, nativePrepared: false, scientificQualification: 'UNVERIFIED' } }));
    expect(html).toContain('TradeTick'); expect(html).toContain('仅供探索性分析'); expect(html).toContain('事件时间代理（未验证）');
    expect(html).not.toContain('>BAR<'); expect(html).toContain('未准备');
  });
  it('retains explicit cohort units, source pin and exact nanosecond coverage in results', () => {
    const html = renderToStaticMarkup(createElement(NativeInputFacts, {
      settings: { modality: 'TRADE_TICK', instruments: ['BTC.SIM'], inputFamily: 'tick-cohort-v1', horizon: 2, horizonUnit: 'TRADE_COHORTS', sampling: 'EVENT_TIME_COHORT_V1', sourceRevision: 'original-revision', partitions: null, strategy: null },
      coverage: { event_start_ns: '900719925474099301', event_end_ns: '900719925474099399', trade_count: '7' },
    }));
    expect(html).toContain('2 个后续成交组'); expect(html).toContain('original-revision');
    expect(html).toContain('900719925474099301'); expect(html).toContain('900719925474099399'); expect(html).not.toContain('次 BAR 观测');
  });
  it('displays event-time duration exactly without relabeling it as cohorts', () => {
    for (const [ns, expected] of [['10000000000', '10 秒'], ['60000000000', '60 秒'], ['9007199254740993', '9007199254740993 ns']]) {
      const html = renderToStaticMarkup(createElement(NativeInputFacts, {
        settings: { modality: 'TRADE_TICK', instruments: ['BTC.SIM'], inputFamily: 'tick-cohort-v1', horizon: null,
          horizonNs: ns, horizonUnit: 'EVENT_TIME_NS', partitions: null, strategy: null },
      }));
      expect(html).toContain(expected); expect(html).toContain('取首个达标成交组');
      expect(html).not.toContain('个后续成交组'); expect(html).not.toContain('次 BAR 观测');
    }
  });

});
