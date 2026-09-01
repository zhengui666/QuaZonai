import { CornersOutIcon, XIcon } from '@phosphor-icons/react';
import { Button, Dialog } from '@radix-ui/themes';
import { useId, useState, type ReactNode } from 'react';
import { useI18n } from '../../i18n';

export interface GraphDataItem {
  id: string;
  label: ReactNode;
  details: Array<[string, ReactNode]>;
}

export function GraphDataList({ items }: { items: GraphDataItem[] }) {
  const { t } = useI18n();
  if (!items.length) return null;
  return (
    <section className="qz-graph-data-list" aria-label={t('mobile.graphList')}>
      <h3>{t('mobile.graphList')}</h3>
      <div className="qz-graph-data-items" role="list">
        {items.map((item) => <article key={item.id} role="listitem" className="qz-graph-data-item"><strong>{item.label}</strong><dl>{item.details.map(([key, value]) => <div key={key}><dt>{key}</dt><dd>{value}</dd></div>)}</dl></article>)}
      </div>
    </section>
  );
}

export function GraphViewport({ ariaLabel, items, children }: { ariaLabel: string; items: GraphDataItem[]; children: ReactNode }) {
  const { t } = useI18n();
  const [expanded, setExpanded] = useState(false);
  const descriptionId = useId();
  return (
    <Dialog.Root open={expanded} onOpenChange={setExpanded}>
      <div className="qz-graph-frame">
        <div className="qz-graph-toolbar">
          <span className="qz-section-meta">{ariaLabel}</span>
          <Dialog.Trigger>
            <Button className="qz-touch-button" size="1" variant="soft">
              <CornersOutIcon size={14} />{t('mobile.expandGraph')}
            </Button>
          </Dialog.Trigger>
        </div>
        {!expanded ? <>
          <div className="qz-graph-canvas">{children}</div>
          <GraphDataList items={items} />
        </> : null}
      </div>
      {expanded ? (
        <Dialog.Content className="qz-graph-dialog" aria-describedby={descriptionId}>
          <div className="qz-graph-toolbar">
            <Dialog.Title>{ariaLabel}</Dialog.Title>
            <Dialog.Close>
              <Button className="qz-touch-button" size="1" variant="soft">
                <XIcon size={14} />{t('mobile.closeGraph')}
              </Button>
            </Dialog.Close>
          </div>
          <Dialog.Description id={descriptionId} className="qz-visually-hidden">{t('mobile.graphDialogDescription')}</Dialog.Description>
          <div className="qz-graph-canvas">{children}</div>
          <GraphDataList items={items} />
        </Dialog.Content>
      ) : null}
    </Dialog.Root>
  );
}
