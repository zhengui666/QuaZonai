import { describe, expect, it } from 'vitest';
import { formatRedundancyScore } from '../components/graphs/RedundancyGraph';

describe('RedundancyGraph score formatting', () => {
  it('preserves small nonzero redundancy scores with locale separators', () => {
    expect(formatRedundancyScore('es', 0.004)).toBe(
      new Intl.NumberFormat('es', { maximumSignificantDigits: 15 }).format(0.004),
    );
  });
});
