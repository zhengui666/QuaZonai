import { Button } from '@radix-ui/themes';
import { Link } from 'react-router-dom';
import { EmptyState } from '../components/ui/EmptyState';
import { useI18n } from '../i18n';

export function NotFoundPage() {
  const { t } = useI18n();
  return <EmptyState title="Page not found" description="This route is not part of the QuaZonai workbench." action={<Button asChild><Link to="/">{t('notFound.home')}</Link></Button>} />;
}
