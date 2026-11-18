import i18n from './i18n';
const t = (key: string, values?: Record<string, unknown>): string => i18n.t(key, { ns: 'settings', ...values });
/** Display provider values without translating or changing the submitted effort. */
export function thinkingLabel(value: string): string {
  const labels: Record<string, string> = { off: 'Off', minimal: 'Minimal', low: 'Low', medium: 'Medium', high: 'High', xhigh: 'XHigh', max: 'Max', ultra: 'Ultra', default: 'Default' };
  return labels[value] ?? value;
}

type ThinkingCapabilities = { thinkingLevels: readonly string[]; reasoningModes?: readonly { level: string; effectiveLevel: string; mode: 'PI_MULTI_AGENT' }[]; unavailableThinkingLevels?: readonly { level: string; reason: string }[] };

/** Advertised but rejected efforts stay visible without becoming selectable. */
export function thinkingOptions(model?: ThinkingCapabilities) {
  const modes = new Set(model?.reasoningModes?.map(item => item.level));
  const unavailable = new Map(model?.unavailableThinkingLevels?.filter(item => !modes.has(item.level)).map(item => [item.level, item.reason]));
  return [...new Set([...(model?.thinkingLevels ?? []), ...modes, ...unavailable.keys()])].map(value => ({
    value, label: thinkingLabel(value), ...(unavailable.has(value) ? { disabled: true, title: unavailable.get(value) } : {}),
  }));
}

export function thinkingUnavailableReason(model?: Partial<ThinkingCapabilities>) {
  return model?.unavailableThinkingLevels?.filter(item => !model.reasoningModes?.some(mode => mode.level === item.level)).map(item => t("{{v0}} is unavailable: {{v1}}", { v0: thinkingLabel(item.level), v1: item.reason })).join(i18n.language === 'zh-CN' ? '；' : '; ') || undefined;
}

export function reasoningModeDescription(model: Partial<ThinkingCapabilities> | undefined, level: string | undefined) {
  const mode = model?.reasoningModes?.find(item => item.level === level);
  return mode ? t("{{v0}} (Pi multi-agent analysis delegation), with underlying model effort {{v1}}. Child agents only analyze, review, and synthesize material supplied by the parent. The main agent owns scientific tools and final integration.", { v0: thinkingLabel(mode.level), v1: thinkingLabel(mode.effectiveLevel) }) : undefined;
}
