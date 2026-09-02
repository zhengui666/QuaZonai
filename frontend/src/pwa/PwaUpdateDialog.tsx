import { ArrowClockwiseIcon } from '@phosphor-icons/react';
import { useIsMutating } from '@tanstack/react-query';
import { Button, Dialog, Flex, Spinner, Text } from '@radix-ui/themes';
import { useI18n } from '../i18n';
import { usePwa } from './PwaProvider';

export function PwaUpdateDialog() {
  const { t } = useI18n();
  const activeMutations = useIsMutating();
  const {
    needRefresh,
    updatePromptOpen,
    updatePhase,
    dismissUpdatePrompt,
    applyUpdate,
  } = usePwa();
  const applying = updatePhase === 'applying';
  const failed = updatePhase === 'failed';
  const mutationInFlight = activeMutations > 0;

  function handleApply() {
    void applyUpdate().catch(() => undefined);
  }

  return (
    <Dialog.Root
      open={needRefresh && updatePromptOpen}
      onOpenChange={(open) => {
        if (!open && !applying) dismissUpdatePrompt();
      }}
    >
      <Dialog.Content className="qz-pwa-update-dialog" aria-describedby="qz-pwa-update-description">
        <Dialog.Title>{t('pwa.updateAvailable')}</Dialog.Title>
        <Dialog.Description id="qz-pwa-update-description" className="qz-pwa-update-copy">
          {t('pwa.updateDescription')}
        </Dialog.Description>
        {mutationInFlight ? <Text className="qz-pwa-update-pending" role="status">{t('pwa.updatePendingMutation')}</Text> : null}
        {failed ? <Text className="qz-pwa-update-error" role="alert">{t('pwa.updateFailed')}</Text> : null}
        <Flex className="qz-pwa-update-actions" justify="end" gap="2" mt="4">
          <Button variant="soft" color="gray" disabled={applying} onClick={dismissUpdatePrompt}>{t('pwa.updateLater')}</Button>
          <Button disabled={applying || mutationInFlight} onClick={handleApply}>
            {applying ? <Spinner /> : <ArrowClockwiseIcon />}
            {applying ? t('pwa.updating') : failed ? t('pwa.updateRetry') : t('pwa.updateNow')}
          </Button>
        </Flex>
      </Dialog.Content>
    </Dialog.Root>
  );
}
