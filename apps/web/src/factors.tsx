import { useTranslation } from 'react-i18next';
/** Provenance identifier only; the retired factor research product is not an active route. */
export function FactorIdentity({ id }: { id: string }) {
  useTranslation('core');
  return <code>{id}</code>;
}
