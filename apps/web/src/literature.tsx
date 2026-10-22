import type { LiteraturePaper, LiteratureSearch, LiteratureSource, LiteratureView } from '../../../packages/contracts/src/index.ts';
import { Empty, Timestamp } from './ui';
import './literature.css';

const sources: Record<LiteratureSource, string> = { crossref: 'Crossref', arxiv: 'arXiv', semanticscholar: 'Semantic Scholar' };
const statuses: Record<LiteratureSearch['status'], string> = { completed: '已检索', partial: '部分来源不可用', unavailable: '检索不可用', no_results: '本次未找到相关结果' };
const relations = { supports: '支持', contradicts: '反对', background: '背景' };

function sourceError(code: string | null) {
  if (code && /429|rate_limit/.test(code)) return '请求频率受限';
  if (code && /timeout/.test(code)) return '访问超时';
  if (code && /too_large/.test(code)) return '来源响应过大';
  if (code && /invalid|parse/.test(code)) return '来源内容无法读取';
  return '暂时无法访问';
}
function sourceUrl(value: string) {
  try {
    const url = new URL(value);
    return url.protocol === 'https:' && !url.username && !url.password ? url.href : undefined;
  } catch { return undefined; }
}
function Paper({ paper, search, value }: { paper: LiteraturePaper; search: LiteratureSearch; value: LiteratureView }) {
  const url = sourceUrl(paper.url);
  const notes = value.notes.filter(note => note.searchId === search.id && note.paperId === paper.id);
  const doi = paper.doi ? `https://doi.org/${paper.doi.split('/').map(encodeURIComponent).join('/')}` : undefined;
  return <article className="literature-paper">
    <h4>{url ? <a href={url} target="_blank" rel="noreferrer noopener">{paper.title}</a> : paper.title}</h4>
    <div className="literature-meta"><span>{sources[paper.source]}</span><span>{paper.year ?? '年份未确认'}</span>{paper.venue && <span>{paper.venue}</span>}</div>
    <p className="literature-authors">{paper.authors.length ? paper.authors.join('、') : '作者未确认'}</p>
    {doi && <a className="text-link literature-doi" href={doi} target="_blank" rel="noreferrer noopener">DOI: {paper.doi}</a>}
    {paper.abstract ? <details className="literature-abstract"><summary>摘要（非全文）</summary><p>{paper.abstract}</p></details> : <p className="quiet">仅有书目信息，未获取摘要。</p>}
    {!!notes.length && <div className="literature-notes">{notes.map(note => <section className="literature-note" key={note.id}>
      <h5>研究论点 · {relations[note.relation]}</h5><p>{note.claim}</p>
      {note.excerpt && <blockquote>{note.excerpt}</blockquote>}
      <p><span className="quiet">适用限制：</span>{note.limitations}</p>
    </section>)}</div>}
  </article>;
}
export function Literature({ value, publicQueries }: { value?: LiteratureView; publicQueries?: string[] }) {
  const topics = <section className="literature-topics"><h3>公开检索主题</h3>{publicQueries?.length ? <ol>{publicQueries.map((query, index) => <li key={index}>{query}</li>)}</ol> : <p className="quiet">未设置公开检索主题</p>}</section>;
  if (!value?.searches.length) return <div className="literature-content">{topics}{publicQueries?.length ? <Empty title="尚未检索文献" /> : null}</div>;
  return <div className="literature-content">
    {topics}
    {value.searches.map(search => <section className="literature-search" key={search.id}>
      <header className="literature-search-heading"><h3>{search.query}</h3><span className="literature-status">{statuses[search.status]}</span></header>
      <div className="literature-meta"><span>检索于 <Timestamp value={search.searchedAt} /></span><span>{search.papers.length} 篇</span></div>
      <ul className="literature-sources" aria-label="检索来源">{search.sources.map(source => <li key={source.source}><span>{sources[source.source]}</span><span className={source.status === 'error' ? 'literature-source-error' : 'quiet'}>{source.status === 'ok' ? '已返回' : sourceError(source.error)}</span></li>)}</ul>
      {search.papers.map(paper => <Paper key={paper.id} paper={paper} search={search} value={value} />)}
      {!search.papers.length && <p className="quiet">{search.status === 'unavailable' ? '来源不可用，尚无可读论文。' : search.status === 'partial' ? '已响应的来源未返回相关论文，其他来源未完成。' : '这些来源本次未返回相关论文。'}</p>}
    </section>)}
  </div>;
}
