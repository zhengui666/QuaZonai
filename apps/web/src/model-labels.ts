/** Display provider values without translating or changing the submitted effort. */
export function thinkingLabel(value: string): string {
  const labels: Record<string, string> = { off: 'Off', minimal: 'Minimal', low: 'Low', medium: 'Medium', high: 'High', xhigh: 'XHigh', max: 'Max', ultra: 'Ultra', default: 'Default' };
  return labels[value] ?? value;
}

/** Advertised but rejected efforts stay visible without becoming selectable. */
export function thinkingOptions(model?: { thinkingLevels: readonly string[]; unavailableThinkingLevels?: readonly { level: string; reason: string }[] }) {
  const unavailable = new Map(model?.unavailableThinkingLevels?.map(item => [item.level, item.reason]));
  return [...new Set([...(model?.thinkingLevels ?? []), ...unavailable.keys()])].map(value => ({
    value, label: thinkingLabel(value), ...(unavailable.has(value) ? { disabled: true, title: unavailable.get(value) } : {}),
  }));
}

export function thinkingUnavailableReason(model?: { unavailableThinkingLevels?: readonly { level: string; reason: string }[] }) {
  return model?.unavailableThinkingLevels?.map(item => `${thinkingLabel(item.level)} 不可用：${item.reason}`).join('；') || undefined;
}
