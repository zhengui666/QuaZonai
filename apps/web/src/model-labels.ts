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
  return model?.unavailableThinkingLevels?.filter(item => !model.reasoningModes?.some(mode => mode.level === item.level)).map(item => `${thinkingLabel(item.level)} 不可用：${item.reason}`).join('；') || undefined;
}

export function reasoningModeDescription(model: Partial<ThinkingCapabilities> | undefined, level: string | undefined) {
  const mode = model?.reasoningModes?.find(item => item.level === level);
  return mode ? `${thinkingLabel(mode.level)}（Pi 多 Agent 分析委派），底层模型配置 ${thinkingLabel(mode.effectiveLevel)}。子 Agent 仅分析、审查和综合父 Agent 提供的材料；科学工具与最终整合由主 Agent 承担。` : undefined;
}
