export const pluralMessageKeys = [
  'table.rows.zero',
  'table.rows.one',
  'table.rows.two',
  'table.rows.few',
  'table.rows.many',
  'table.rows.other',
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
};
