import { TrayIcon } from '@phosphor-icons/react';
import type { ReactNode } from 'react';
import { useI18n } from '../../i18n';

export function EmptyState({ title, description, action }: { title: string; description: string; action?: ReactNode }) {
  const { text } = useI18n();
  return <div className="qz-empty"><div><TrayIcon size={24} aria-hidden /><strong>{text(title)}</strong><div>{text(description)}</div>{action ? <div style={{ marginTop: 12 }}>{action}</div> : null}</div></div>;
}
