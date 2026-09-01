import { Button, Callout } from '@radix-ui/themes';
import { ArrowClockwiseIcon } from '@phosphor-icons/react';
import { useI18n } from '../i18n';
import { usePwa } from './PwaProvider';

export function PwaUpdateBanner() {
  const { t } = useI18n();
  const { needRefresh, applyUpdate } = usePwa();
  if (!needRefresh) return null;
  return (
    <Callout.Root className="qz-pwa-banner" role="status" size="1" color="jade">
      <Callout.Icon><ArrowClockwiseIcon /></Callout.Icon>
      <Callout.Text>{t('pwa.updateAvailable')}</Callout.Text>
      <Button size="1" variant="soft" onClick={() => { void applyUpdate(); }}>{t('pwa.updateNow')}</Button>
    </Callout.Root>
  );
}
