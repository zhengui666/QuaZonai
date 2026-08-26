import { WarningCircleIcon } from '@phosphor-icons/react';
import type { ReactNode } from 'react';
import { useI18n } from '../../i18n';
import { ApiError } from '../../lib/api/client';

export function ErrorPanel({ error, action }: { error: unknown; action?: ReactNode }) {
  const { t } = useI18n();
  const message = error instanceof Error ? error.message : t('error.unexpected');
  const code = error instanceof ApiError ? error.code : undefined;
  return <div className="qz-error"><div><WarningCircleIcon size={24} aria-hidden /><strong>{code ? <bdi dir="auto">{code}</bdi> : t('error.unableLoad')}</strong><div dir="auto">{message}</div>{action ? <div style={{ marginTop: 12 }}>{action}</div> : null}</div></div>;
}
