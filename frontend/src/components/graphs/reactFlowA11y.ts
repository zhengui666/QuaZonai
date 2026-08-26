import type { AriaLabelConfig } from '@xyflow/react';
import { useMemo } from 'react';
import { useI18n } from '../../i18n';

export function useReactFlowAriaLabelConfig(): Partial<AriaLabelConfig> {
  const { t } = useI18n();
  return useMemo(() => ({
    'controls.ariaLabel': t('a11y.flowControls'),
    'controls.zoomIn.ariaLabel': t('a11y.flowZoomIn'),
    'controls.zoomOut.ariaLabel': t('a11y.flowZoomOut'),
    'controls.fitView.ariaLabel': t('a11y.flowFitView'),
  }), [t]);
}
