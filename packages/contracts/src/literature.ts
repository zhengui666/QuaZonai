/** Retrieved public literature is research context, separate from scientific evidence. */
export type LiteratureSource = 'crossref' | 'arxiv' | 'semanticscholar';
export type LiteraturePaper = {
  id: string; source: LiteratureSource; title: string; authors: string[];
  year: number | null; venue: string | null; doi: string | null; url: string;
  abstract: string | null; publishedAt: string | null;
};
export type LiteratureSearchResult = {
  query: string; searchedAt: string;
  status: 'completed' | 'partial' | 'unavailable' | 'no_results';
  sources: { source: LiteratureSource; status: 'ok' | 'error'; error: string | null }[];
  papers: LiteraturePaper[];
};
export type LiteratureSearch = LiteratureSearchResult & { id: number };
export type LiteratureNote = {
  id: number; searchId: number; paperId: string; claim: string;
  relation: 'supports' | 'contradicts' | 'background'; limitations: string;
  excerpt: string | null;
};
export type LiteratureView = { searches: LiteratureSearch[]; notes: LiteratureNote[] };
