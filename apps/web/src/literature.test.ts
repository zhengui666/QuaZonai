import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { LiteratureView } from '../../../packages/contracts/src/index.ts';
import { Literature } from './literature';

const literature: LiteratureView = { searches: [{
  id: 7, query: 'momentum and market efficiency', searchedAt: '2026-10-22T04:05:06Z', status: 'partial',
  sources: [{ source: 'crossref', status: 'ok', error: null }, { source: 'arxiv', status: 'error', error: 'http_429' }],
  papers: [{ id: 'doi:10.1000/example', source: 'crossref', title: 'A test paper <script>', authors: ['A. Researcher'], year: 2024, venue: 'Test journal',
    doi: '10.1000/example', url: 'https://example.org/paper', abstract: 'Momentum depends on transaction costs.', publishedAt: null }],
}], notes: [{ id: 8, searchId: 7, paperId: 'doi:10.1000/example', relation: 'contradicts', claim: 'Costs can remove the reported effect.', limitations: 'The tested market differs from this study.', excerpt: 'depends on transaction costs' }] };

describe('literature research presentation', () => {
  it('shows retrievable sources, partial failures and original claims without treating abstracts as full text', () => {
    const markup = renderToStaticMarkup(createElement(Literature, { value: literature }));
    expect(markup).toContain('部分来源不可用');
    expect(markup).toContain('请求频率受限');
    expect(markup).toContain('dateTime="2026-10-22T04:05:06Z"');
    expect(markup).toContain('href="https://example.org/paper"');
    expect(markup).toContain('rel="noreferrer noopener"');
    expect(markup).toContain('A test paper &lt;script&gt;');
    expect(markup).toContain('摘要（非全文）');
    expect(markup).toContain('研究论点 · 反对');
    expect(markup).toContain('The tested market differs from this study.');
    expect(markup).not.toContain(' open=');
  });
  it('keeps older studies empty and never turns untrusted source URLs into active links', () => {
    expect(renderToStaticMarkup(createElement(Literature))).toContain('尚未检索文献');
    const unsafe = structuredClone(literature);
    unsafe.searches[0]!.papers[0]!.url = 'javascript:alert(1)';
    const markup = renderToStaticMarkup(createElement(Literature, { value: unsafe }));
    expect(markup).not.toContain('javascript:');
    expect(markup).toContain('A test paper &lt;script&gt;');
    expect(markup).toContain('href="https://doi.org/10.1000/example"');
  });
});
