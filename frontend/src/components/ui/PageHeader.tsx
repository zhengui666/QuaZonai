import type { ReactNode } from 'react';
import { useI18n } from '../../i18n';

export function PageHeader({ title, description, actions, translateTitle = true, translateDescription = true }: { title: ReactNode; description?: string; actions?: ReactNode; translateTitle?: boolean; translateDescription?: boolean }) {
  const { text } = useI18n();
  const renderedTitle = translateTitle && typeof title === 'string' ? text(title) : title;
  const renderedDescription = description && translateDescription ? text(description) : description;
  return <header className="qz-page-header"><div><h1 className="qz-page-title" dir={translateTitle && typeof title === 'string' ? undefined : 'auto'}>{renderedTitle}</h1>{renderedDescription ? <p className="qz-page-description" dir={translateDescription ? undefined : 'auto'}>{renderedDescription}</p> : null}</div>{actions ? <div className="qz-page-actions">{actions}</div> : null}</header>;
}
