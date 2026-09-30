import { describe, expect, it } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import { createElement } from 'react';
import { Button, ConfigProvider } from 'antd';
import { buttonConfig } from './button-config';

describe('shared official button loading configuration', () => {
  it('keeps the native loading glyph decorative without changing action text', () => {
    const markup = renderToStaticMarkup(createElement(ConfigProvider, { button: buttonConfig },
      createElement(Button, { loading: true, 'aria-busy': true }, '任意操作')));
    const icon = markup.match(/<span[^>]*role="img"[^>]*>/g);
    expect(icon).toHaveLength(1);
    expect(icon![0]).toContain('aria-label="loading"');
    expect(icon![0]).toContain('aria-hidden="true"');
    expect(markup).toContain('ant-btn-loading');
    expect(markup).toContain('aria-busy="true"');
    expect(markup).toContain('任意操作');
  });

  it('does not add a loading label or glyph to an idle action', () => {
    const markup = renderToStaticMarkup(createElement(ConfigProvider, { button: buttonConfig },
      createElement(Button, { loading: false }, '继续')));
    expect(markup).not.toContain('aria-label="loading"');
    expect(markup).not.toContain('ant-btn-loading-icon');
    expect(markup).toContain('继续');
    expect(markup).not.toContain('继 续');
  });
});
