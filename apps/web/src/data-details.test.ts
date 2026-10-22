import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { DataDetails } from './data-details';

const render = (value: unknown, labels?: Record<string, string>) => renderToStaticMarkup(createElement(DataDetails, { value, labels, label: '字段详情' }));

describe('readable saved data', () => {
  it('retains nested fields, ordered entries and explicit empty values', () => {
    const html = render({ name: '组合甲', enabled: true, notes: '', missing: null, entries: [{ id: 'first' }, { id: 'second' }], empty: [] }, { entries: '成员' });
    for (const text of ['组合甲', '是', '空文本', '无值', '无条目', '成员', 'first', 'second']) expect(html).toContain(text);
    expect(html.indexOf('>first<')).toBeLessThan(html.indexOf('>second<'));
    expect(html).not.toContain('<pre');
    expect(html).not.toContain('&quot;name&quot;');
  });

  it('preserves exact historical integer, decimal and exponent lexemes', () => {
    const html = render('{"count":900719925474099312345,"price":0.12345678901234567890,"scale":1.2300e-18,"id":"00123"}');
    for (const value of ['900719925474099312345', '0.12345678901234567890', '1.2300e-18', '00123']) expect(html).toContain(value);
    expect(html).not.toContain('900719925474099400000');
  });

  it('expands serialized nested fields and line-delimited records', () => {
    const html = render({ parameters: '{"adapter":"Native","rows":[{"value":12}]}' });
    expect(html).toContain('Native');
    expect(html).toContain('12');
    expect(html).not.toContain('&quot;adapter&quot;');
    const records = render('```jsonl\n{"id":"one"}\n{"id":"two"}\n```');
    expect(records).toContain('one');
    expect(records).toContain('two');
    expect(records).not.toContain('```');
    expect(records).not.toContain('&quot;id&quot;');
  });

  it.each(['{"value":', '{"value":01}', '[{"id":1},', '```json\n{"secret":\n```'])(
    'does not expose malformed or partial serialized text: %s', value => {
      const html = render(value);
      expect(html).toContain('此内容暂时无法整理为完整字段');
      expect(html).not.toContain('&quot;value&quot;');
      expect(html).not.toContain('&quot;secret&quot;');
    },
  );

  it('keeps plain text and escapes untrusted markup', () => {
    const html = render('常规说明\n<script>alert(1)</script>');
    expect(html).toContain('常规说明');
    expect(html).toContain('&lt;script&gt;');
    expect(html).not.toContain('<script>');
    for (const text of ['[NOTICE] 数据缺失', '{name} 未配置', '"hello" is a note']) {
      expect(render(text)).not.toContain('无法整理');
      expect(render(text)).toContain(text.replaceAll('"', '&quot;'));
    }
  });

  it('renders inline serialized content as readable prose, including field names', () => {
    const html = render({ note: '运行信息：{"message":"已完成","count":9007199254740993}', '{"id":"source"}': '备注' });
    expect(html).toContain('已完成');
    expect(html).toContain('9007199254740993');
    expect(html).toContain('source');
    expect(html).not.toContain('&quot;message&quot;');
    expect(html).not.toContain('&quot;id&quot;');
  });

  it('retains fields whose names also exist on Object.prototype', () => {
    const html = render('{"__proto__":{"id":"legacy"},"constructor":"custom-model","toString":"source-text"}');
    for (const value of ['__proto__', 'legacy', 'constructor', 'custom-model', 'toString', 'source-text']) expect(html).toContain(value);
    expect(html).not.toContain('function Object');
  });
});
