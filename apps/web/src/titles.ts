/** Deterministic display copy only; callers retain the full original record. */
export function shortTitle(name?: string | null, text?: string | null, fallback = '未命名', limit = 42): string {
  const explicit = name?.trim();
  const source = explicit || text?.split(/\n/).map(line => line.replace(/^\s*(?:#{1,6}\s+|[-*]\s+)/, '').trim()).find(Boolean) || fallback;
  const sentence = explicit ? source : source.split(/[。！？!?]|\.\s+/)[0] || source;
  const clean = sentence.replace(/\s+/g, ' ').trim(); const characters = Array.from(clean);
  return characters.length > limit ? `${characters.slice(0, limit - 1).join('')}…` : clean;
}
export function shouldCollapse(value: string, limit = 100): boolean { return Array.from(value).length > limit || value.includes('\n'); }
