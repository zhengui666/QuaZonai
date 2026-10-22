import { publicLiteratureQueries } from '../../../packages/contracts/src/research.ts';

/** Preserve each owner-written topic; omit an empty optional text area. */
export function publicLiteratureTopics(value?: string) {
  const topics = value?.split(/\r?\n/).filter(topic => topic.trim());
  return topics?.length ? publicLiteratureQueries(topics) : undefined;
}
