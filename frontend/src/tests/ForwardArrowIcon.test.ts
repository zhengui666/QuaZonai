import { describe, expect, it } from 'vitest';
import { forwardArrowDirection } from '../components/ui/ForwardArrowIcon';

describe('ForwardArrowIcon', () => {
  it('mirrors forward navigation only for RTL locales', () => {
    expect(forwardArrowDirection('ar')).toBe('left');
    expect(forwardArrowDirection('en')).toBe('right');
    expect(forwardArrowDirection('zh-CN')).toBe('right');
  });
});
