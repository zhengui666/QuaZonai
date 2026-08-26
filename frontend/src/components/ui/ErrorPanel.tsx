import { WarningCircleIcon } from '@phosphor-icons/react';
import type { ReactNode } from 'react';
import { useI18n } from '../../i18n';
import { ApiError } from '../../lib/api/client';

export function ErrorPanel({ error, action }: { error: unknown; action?: ReactNode }) {
  const { t } = useI18n();
  const apiError = error instanceof ApiError ? error : undefined;
  const message = apiError?.failure.kind === 'api'
    ? apiError.failure.message
    : apiError?.failure.kind === 'http'
      ? t('error.requestHttpError', { status: apiError.failure.status })
      : apiError?.failure.kind === 'network'
        ? t('error.requestUnreachable')
        : error instanceof Error
          ? error.message
          : t('error.unexpected');
  const code = apiError?.code;
  return <div className="qz-error"><div><WarningCircleIcon size={24} aria-hidden /><strong>{code ? <bdi dir="auto">{code}</bdi> : t('error.unableLoad')}</strong><div dir="auto">{message}</div>{action ? <div style={{ marginTop: 12 }}>{action}</div> : null}</div></div>;
}
