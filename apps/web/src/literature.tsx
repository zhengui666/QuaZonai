import { useTranslation } from 'react-i18next';
import { tr } from './i18n';
import type { LiteraturePaper, LiteratureSearch, LiteratureSource, LiteratureView } from '../../../packages/contracts/src/index.ts';
import { Empty, Timestamp } from './ui';
import './literature.css';

const sources: Record<LiteratureSource, string> = { crossref: 'Crossref', arxiv: 'arXiv', semanticscholar: 'Semantic Scholar' };
const statuses: Record<LiteratureSearch['status'], string> = { completed: "Search completed", partial: "Some sources unavailable", unavailable: "Search unavailable", no_results: "No relevant results found" };
const relations = { supports: "Supports", contradicts: "Challenges", background: "Background" };

function sourceError(code: string | null) {
  if (code && /429|rate_limit/.test(code)) return tr("Rate limited");
  if (code && /timeout/.test(code)) return tr("Request timed out");
  if (code && /too_large/.test(code)) return tr("Source response too large");
  if (code && /invalid|parse/.test(code)) return tr("Source content could not be read");
  return tr("Temporarily unavailable");
}
function sourceUrl(value: string) {
  try {
    const url = new URL(value);
    return url.protocol === 'https:' && !url.username && !url.password ? url.href : undefined;
  } catch { return undefined; }
}
function Paper({ paper, search, value }: { paper: LiteraturePaper; search: LiteratureSearch; value: LiteratureView }) {
  useTranslation('core');
  const url = sourceUrl(paper.url);
  const notes = value.notes.filter(note => note.searchId === search.id && note.paperId === paper.id);
  const doi = paper.doi ? `https://doi.org/${paper.doi.split('/').map(encodeURIComponent).join('/')}` : undefined;
  return <article className="literature-paper">
    <h4>{url ? <a href={url} target="_blank" rel="noreferrer noopener">{paper.title}</a> : paper.title}</h4>
    <div className="literature-meta"><span>{sources[paper.source]}</span><span>{paper.year ?? tr("Year unconfirmed")}</span>{paper.venue && <span>{paper.venue}</span>}</div>
    <p className="literature-authors">{paper.authors.length ? paper.authors.join('、') : tr("Authors unconfirmed")}</p>
    {doi && <a className="text-link literature-doi" href={doi} target="_blank" rel="noreferrer noopener">DOI: {paper.doi}</a>}
    {paper.abstract ? <details className="literature-abstract"><summary>{tr("Abstract (not full text)")}</summary><p>{paper.abstract}</p></details> : <p className="quiet">{tr("Bibliographic information only; no abstract was retrieved.")}</p>}
    {!!notes.length && <div className="literature-notes">{notes.map(note => <section className="literature-note" key={note.id}>
      <h5>{tr("Research claim ·")} {tr(relations[note.relation])}</h5><p>{note.claim}</p>
      {note.excerpt && <blockquote>{note.excerpt}</blockquote>}
      <p><span className="quiet">{tr("Limitations:")}</span>{note.limitations}</p>
    </section>)}</div>}
  </article>;
}
export function Literature({ value, publicQueries }: { value?: LiteratureView; publicQueries?: string[] }) {
  useTranslation('core');
  const topics = <section className="literature-topics"><h3>{tr("Public search topics")}</h3>{publicQueries?.length ? <ol>{publicQueries.map((query, index) => <li key={index}>{query}</li>)}</ol> : <p className="quiet">{tr("No public search topics set")}</p>}</section>;
  if (!value?.searches.length) return <div className="literature-content">{topics}{publicQueries?.length ? <Empty title={tr("Literature has not been searched")} /> : null}</div>;
  return <div className="literature-content">
    {topics}
    {value.searches.map(search => <section className="literature-search" key={search.id}>
      <header className="literature-search-heading"><h3>{search.query}</h3><span className="literature-status">{tr(statuses[search.status])}</span></header>
      <div className="literature-meta"><span>{tr("Searched")} <Timestamp value={search.searchedAt} /></span><span>{search.papers.length}  {tr("papers")}</span></div>
      <ul className="literature-sources" aria-label={tr("Search sources")}>{search.sources.map(source => <li key={source.source}><span>{sources[source.source]}</span><span className={source.status === 'error' ? 'literature-source-error' : 'quiet'}>{source.status === 'ok' ? tr("Returned") : sourceError(source.error)}</span></li>)}</ul>
      {search.papers.map(paper => <Paper key={paper.id} paper={paper} search={search} value={value} />)}
      {!search.papers.length && <p className="quiet">{search.status === 'unavailable' ? tr("Sources are unavailable; no papers can be read yet.") : search.status === 'partial' ? tr("The responding sources returned no relevant papers. Other sources have not completed.") : tr("These sources returned no relevant papers in this search.")}</p>}
    </section>)}
  </div>;
}
