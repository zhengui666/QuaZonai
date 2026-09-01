import { WifiHighIcon, WifiSlashIcon } from '@phosphor-icons/react';
import { Callout } from '@radix-ui/themes';
import { useEffect, useRef, useState } from 'react';
import { useI18n } from '../i18n';
import { usePwa } from './PwaProvider';

export function OnlineStatusBanner() {
  const { t } = useI18n();
  const { isOnline, offlineReady } = usePwa();
  const [showReconnected, setShowReconnected] = useState(false);
  const wasOffline = useRef(!isOnline);
  useEffect(() => {
    if (!isOnline) {
      wasOffline.current = true;
      setShowReconnected(false);
      return undefined;
    }
    if (!wasOffline.current) {
      setShowReconnected(false);
      return undefined;
    }
    wasOffline.current = false;
    setShowReconnected(true);
    const timeout = window.setTimeout(() => setShowReconnected(false), 4_000);
    return () => window.clearTimeout(timeout);
  }, [isOnline]);
  if (!isOnline) {
    return <Callout.Root className="qz-pwa-banner" role="status" size="1" color="amber"><Callout.Icon><WifiSlashIcon /></Callout.Icon><Callout.Text>{t('pwa.offline')} {t('pwa.requiresConnection')}</Callout.Text></Callout.Root>;
  }
  if (showReconnected && offlineReady) {
    return <Callout.Root className="qz-pwa-banner" role="status" size="1" color="green"><Callout.Icon><WifiHighIcon /></Callout.Icon><Callout.Text>{t('pwa.reconnected')}</Callout.Text></Callout.Root>;
  }
  return null;
}
