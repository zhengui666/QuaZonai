import { Dialog } from '@radix-ui/themes';
import type { ComponentProps } from 'react';

export function ResponsiveDialogContent(props: ComponentProps<typeof Dialog.Content>) {
  return <Dialog.Content {...props} className={['qz-responsive-dialog', props.className].filter(Boolean).join(' ')} />;
}
