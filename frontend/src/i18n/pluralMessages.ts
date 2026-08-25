export const pluralMessageKeys = [
  'table.rows.zero',
  'table.rows.one',
  'table.rows.two',
  'table.rows.few',
  'table.rows.many',
  'table.rows.other',
  'home.decisions.zero',
  'home.decisions.one',
  'home.decisions.two',
  'home.decisions.few',
  'home.decisions.many',
  'home.decisions.other',
] as const;

export type PluralMessageKey = (typeof pluralMessageKeys)[number];

type PluralMessageTuple = readonly [string, string, string, string, string, string, string];
const m = (...items: PluralMessageTuple): PluralMessageTuple => items;

export const pluralMessages: Record<PluralMessageKey, PluralMessageTuple> = {
  'table.rows.zero': m('{count} rows', '{count} 行', '{count} 列', '{count} 行', '{count}행', '{count} filas', 'لا صفوف'),
  'table.rows.one': m('{count} row', '{count} 行', '{count} 列', '{count} 行', '{count}행', '{count} fila', 'صف واحد'),
  'table.rows.two': m('{count} rows', '{count} 行', '{count} 列', '{count} 行', '{count}행', '{count} filas', 'صفّان'),
  'table.rows.few': m('{count} rows', '{count} 行', '{count} 列', '{count} 行', '{count}행', '{count} filas', '{count} صفوف'),
  'table.rows.many': m('{count} rows', '{count} 行', '{count} 列', '{count} 行', '{count}행', '{count} filas', '{count} صفًا'),
  'table.rows.other': m('{count} rows', '{count} 行', '{count} 列', '{count} 行', '{count}행', '{count} filas', '{count} صف'),
  'home.decisions.zero': m('{count} decisions', '{count} 个决策', '{count} 個決策', '判断 {count} 件', '의사결정 {count}건', '{count} decisiones', 'لا قرارات'),
  'home.decisions.one': m('{count} decision', '{count} 个决策', '{count} 個決策', '判断 {count} 件', '의사결정 {count}건', '{count} decisión', 'قرار واحد'),
  'home.decisions.two': m('{count} decisions', '{count} 个决策', '{count} 個決策', '判断 {count} 件', '의사결정 {count}건', '{count} decisiones', 'قراران'),
  'home.decisions.few': m('{count} decisions', '{count} 个决策', '{count} 個決策', '判断 {count} 件', '의사결정 {count}건', '{count} decisiones', '{count} قرارات'),
  'home.decisions.many': m('{count} decisions', '{count} 个决策', '{count} 個決策', '判断 {count} 件', '의사결정 {count}건', '{count} decisiones', '{count} قرارًا'),
  'home.decisions.other': m('{count} decisions', '{count} 个决策', '{count} 個決策', '判断 {count} 件', '의사결정 {count}건', '{count} decisiones', '{count} قرار'),
};
