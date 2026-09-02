import { Button, Dialog, Slider, Switch, TextField } from '@radix-ui/themes';
import { useEffect, useMemo, useRef, useState } from 'react';
import { useI18n } from '../../i18n';
import { useCodexChatgptAuth, useUpdateRuntimeConfiguration } from '../../lib/api/hooks';
import { ApiError } from '../../lib/api/client';
import type { CodexChatgptAuthStatus, CodexReasoningEffort, RuntimeConfiguration, RuntimeConfigurationUpdate } from '../../lib/api/types';
import { formatNumber } from '../../lib/format';
import { ErrorPanel } from '../ui/ErrorPanel';
import { ResponsiveDialogContent } from '../ui/ResponsiveDialogContent';
import { Section } from '../ui/Section';
import { StateBadge } from '../ui/StateBadge';

const MAX_PLUGIN_WHEEL_BYTES = 1_073_741_824;
const MAX_WORKER_TIMEOUT_SECONDS = 86_400;
const MAX_JOB_POLL_SECONDS = 3600;
const MAX_JOB_LEASE_SECONDS = 86_400;

type RuntimeConfigurationWithCodexDefaults = RuntimeConfiguration & {
  codex_use_default_model_settings?: boolean;
};

type RuntimeConfigurationUpdateWithCodexDefaults = RuntimeConfigurationUpdate & {
  codex_use_default_model_settings: boolean;
};

export const REASONING_EFFORT_STEPS: ReadonlyArray<{
  value: CodexReasoningEffort | null;
  key: 'runtime.reasoningDefault' | 'runtime.reasoningMinimal' | 'runtime.reasoningLow' | 'runtime.reasoningMedium' | 'runtime.reasoningHigh' | 'runtime.reasoningXHigh';
}> = [
  { value: null, key: 'runtime.reasoningDefault' },
  { value: 'minimal', key: 'runtime.reasoningMinimal' },
  { value: 'low', key: 'runtime.reasoningLow' },
  { value: 'medium', key: 'runtime.reasoningMedium' },
  { value: 'high', key: 'runtime.reasoningHigh' },
  { value: 'xhigh', key: 'runtime.reasoningXHigh' },
];

function positiveNumber(value: string): number {
  const parsed = Number(value);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : 0;
}

function positiveInteger(value: string): number {
  const parsed = Number(value);
  return Number.isSafeInteger(parsed) && parsed > 0 ? parsed : 0;
}

function normalizeBaseUrl(value: string): string {
  return value.trim().replace(/\/+$/, '');
}

function usesCodexModelDefaults(configuration: RuntimeConfigurationWithCodexDefaults): boolean {
  return configuration.codex_use_default_model_settings ?? (
    !configuration.codex_model
    && configuration.codex_reasoning_effort === null
    && !configuration.codex_fast_mode
  );
}

function chatgptState(status: CodexChatgptAuthStatus | undefined, configuration: RuntimeConfiguration): string {
  if (status?.state === 'CONNECTED') return status.active ? 'CONNECTED' : 'INACTIVE';
  if (status?.state === 'REAUTH_REQUIRED') return 'REAUTH_REQUIRED';
  if (configuration.codex_api_key_configured || configuration.codex_base_url) return 'CUSTOM_PROVIDER';
  return 'DISCONNECTED';
}

function ChatgptAuthControls({ configuration }: { configuration: RuntimeConfiguration }) {
  const { t } = useI18n();
  const { auth, deviceLogin, pollResult, start, poll, cancel, disconnect } = useCodexChatgptAuth();
  const [copied, setCopied] = useState(false);
  const status = auth.data;
  const state = chatgptState(status, configuration);
  const startLogin = () => { setCopied(false); start.mutate(); };
  const closeLogin = () => {
    if (deviceLogin) cancel.mutate(deviceLogin.login_id);
  };
  const pollError = pollResult?.status === 'EXPIRED'
    ? new ApiError({ kind: 'api', message: t('runtime.deviceCodeExpired') }, 400, pollResult.error_code ?? 'expired')
    : pollResult?.status === 'FAILED'
      ? new ApiError({ kind: 'api', message: t('runtime.deviceCodeFailed', { code: pollResult.error_code ?? 'unknown' }) }, 400, pollResult.error_code ?? 'authorization_failed')
      : null;

  return (
    <div className="qz-field" style={{ gridColumn: '1 / -1' }}>
      <div style={{ display: 'flex', gap: 10, alignItems: 'center', flexWrap: 'wrap' }}>
        <StateBadge state={state} />
        {state === 'CONNECTED' && status?.email ? <span className="qz-list-subtitle"><bdi dir="ltr">{status.email}</bdi></span> : null}
        {status?.plan_type ? <span className="qz-list-subtitle"><bdi dir="ltr">{status.plan_type}</bdi></span> : null}
      </div>
      {state === 'INACTIVE' ? <span className="qz-help">{t('runtime.chatgptInactive')}</span> : null}
      {state === 'REAUTH_REQUIRED' ? <span className="qz-help">{t('runtime.chatgptReauthRequired')}</span> : null}
      {state === 'CONNECTED' && status?.last_refresh_at ? <span className="qz-list-subtitle">{t('runtime.chatgptLastRefresh')}: <bdi dir="ltr">{status.last_refresh_at}</bdi></span> : null}
      <div style={{ display: 'flex', gap: 10, marginTop: 10, flexWrap: 'wrap' }}>
        {state === 'CONNECTED' || state === 'INACTIVE'
          ? <Button size="2" variant="soft" color="red" disabled={disconnect.isPending} onClick={() => disconnect.mutate()}>{t('runtime.disconnectChatgpt')}</Button>
          : <Button size="2" variant="soft" disabled={start.isPending || Boolean(deviceLogin)} onClick={startLogin}>{state === 'REAUTH_REQUIRED' ? t('runtime.reauthenticateChatgpt') : t('runtime.signInChatgpt')}</Button>}
      </div>
      {start.error ? <div style={{ marginTop: 10 }}><ErrorPanel error={start.error} /></div> : null}
      {pollError ? <div style={{ marginTop: 10 }}><ErrorPanel error={pollError} /></div> : null}
      {poll.error ? <div style={{ marginTop: 10 }}><ErrorPanel error={poll.error} /></div> : null}
      {disconnect.error ? <div style={{ marginTop: 10 }}><ErrorPanel error={disconnect.error} /></div> : null}
      <Dialog.Root open={Boolean(deviceLogin)} onOpenChange={(open) => { if (!open) closeLogin(); }}>
        <ResponsiveDialogContent aria-describedby="codex-device-description">
          <Dialog.Title>{t('runtime.deviceCodeTitle')}</Dialog.Title>
          <Dialog.Description id="codex-device-description">{t('runtime.deviceCodeDescription')}</Dialog.Description>
          {deviceLogin ? (
            <div style={{ display: 'grid', gap: 14, marginTop: 16 }}>
              <a href={deviceLogin.verification_url} target="_blank" rel="noreferrer" dir="ltr">{t('runtime.deviceCodeOpen')}</a>
              <div className="qz-panel qz-panel-pad" style={{ textAlign: 'center' }}>
                <div className="qz-label">{t('runtime.deviceCodeWaiting')}</div>
                <div className="qz-mono" dir="ltr" style={{ fontSize: 'clamp(1.5rem, 8vw, 2.5rem)', letterSpacing: '0.12em', marginTop: 8 }}>{deviceLogin.user_code}</div>
                <Button size="2" variant="soft" style={{ marginTop: 12 }} onClick={() => { void navigator.clipboard?.writeText(deviceLogin.user_code); setCopied(true); }}>{copied ? t('runtime.deviceCodeCopied') : t('runtime.deviceCodeCopy')}</Button>
              </div>
              <span className="qz-help">{t('runtime.deviceCodeExpires', { expires: deviceLogin.expires_at })}</span>
              <span className="qz-list-subtitle" aria-live="polite">{t('runtime.deviceCodePolling', { seconds: deviceLogin.poll_after_seconds })}</span>
              {cancel.error ? <ErrorPanel error={cancel.error} /> : null}
              <Button variant="soft" color="gray" onClick={closeLogin}>{t('runtime.deviceCodeCancel')}</Button>
            </div>
          ) : null}
        </ResponsiveDialogContent>
      </Dialog.Root>
    </div>
  );
}

export function RuntimeConfigurationPanel({ configuration }: { configuration: RuntimeConfigurationWithCodexDefaults }) {
  const { t } = useI18n();
  const update = useUpdateRuntimeConfiguration();
  const reasoningSliderRef = useRef<HTMLSpanElement>(null);
  const [useCodexDefaults, setUseCodexDefaults] = useState(usesCodexModelDefaults(configuration));
  const [model, setModel] = useState(configuration.codex_model ?? '');
  const [reasoningEffort, setReasoningEffort] = useState<CodexReasoningEffort | null>(configuration.codex_reasoning_effort);
  const [fastMode, setFastMode] = useState(configuration.codex_fast_mode);
  const [baseUrl, setBaseUrl] = useState(configuration.codex_base_url ?? '');
  const [apiKey, setApiKey] = useState('');
  const [clearApiKey, setClearApiKey] = useState(false);
  const [maxWheelBytes, setMaxWheelBytes] = useState(String(configuration.max_plugin_wheel_bytes));
  const [pluginValidationTimeout, setPluginValidationTimeout] = useState(String(configuration.plugin_validation_timeout_seconds));
  const [bundleBuildTimeout, setBundleBuildTimeout] = useState(String(configuration.bundle_build_timeout_seconds));
  const [pluginJobTimeout, setPluginJobTimeout] = useState(String(configuration.plugin_job_timeout_seconds));
  const [missionJobTimeout, setMissionJobTimeout] = useState(String(configuration.mission_job_timeout_seconds));
  const [jobPollSeconds, setJobPollSeconds] = useState(String(configuration.job_poll_seconds));
  const [jobLeaseSeconds, setJobLeaseSeconds] = useState(String(configuration.job_lease_seconds));

  useEffect(() => {
    setUseCodexDefaults(usesCodexModelDefaults(configuration));
    setModel(configuration.codex_model ?? '');
    setReasoningEffort(configuration.codex_reasoning_effort);
    setFastMode(configuration.codex_fast_mode);
    setBaseUrl(configuration.codex_base_url ?? '');
    setMaxWheelBytes(String(configuration.max_plugin_wheel_bytes));
    setPluginValidationTimeout(String(configuration.plugin_validation_timeout_seconds));
    setBundleBuildTimeout(String(configuration.bundle_build_timeout_seconds));
    setPluginJobTimeout(String(configuration.plugin_job_timeout_seconds));
    setMissionJobTimeout(String(configuration.mission_job_timeout_seconds));
    setJobPollSeconds(String(configuration.job_poll_seconds));
    setJobLeaseSeconds(String(configuration.job_lease_seconds));
  }, [configuration]);

  const numericValues = useMemo(() => ({
    max_plugin_wheel_bytes: positiveInteger(maxWheelBytes),
    plugin_validation_timeout_seconds: positiveInteger(pluginValidationTimeout),
    bundle_build_timeout_seconds: positiveInteger(bundleBuildTimeout),
    plugin_job_timeout_seconds: positiveInteger(pluginJobTimeout),
    mission_job_timeout_seconds: positiveInteger(missionJobTimeout),
    job_poll_seconds: positiveNumber(jobPollSeconds),
    job_lease_seconds: positiveInteger(jobLeaseSeconds),
  }), [maxWheelBytes, pluginValidationTimeout, bundleBuildTimeout, pluginJobTimeout, missionJobTimeout, jobPollSeconds, jobLeaseSeconds]);

  const selectedReasoningIndex = REASONING_EFFORT_STEPS.findIndex((step) => step.value === reasoningEffort);
  const reasoningInvalid = selectedReasoningIndex < 0;
  const reasoningLabel = reasoningInvalid
    ? ''
    : t(REASONING_EFFORT_STEPS[selectedReasoningIndex].key);

  useEffect(() => {
    const thumb = reasoningSliderRef.current?.querySelector<HTMLElement>('[role="slider"]');
    if (thumb) thumb.setAttribute('aria-valuetext', reasoningLabel);
  }, [reasoningLabel]);

  const providerUrlChanged = normalizeBaseUrl(baseUrl) !== normalizeBaseUrl(configuration.codex_base_url ?? '');
  const requiresKeyDecision = providerUrlChanged && configuration.codex_api_key_configured && !apiKey.trim() && !clearApiKey;
  const invalid = Object.values(numericValues).some((value) => value <= 0)
    || numericValues.max_plugin_wheel_bytes > MAX_PLUGIN_WHEEL_BYTES
    || numericValues.plugin_validation_timeout_seconds > MAX_WORKER_TIMEOUT_SECONDS
    || numericValues.bundle_build_timeout_seconds > MAX_WORKER_TIMEOUT_SECONDS
    || numericValues.plugin_job_timeout_seconds > MAX_WORKER_TIMEOUT_SECONDS
    || numericValues.mission_job_timeout_seconds > MAX_WORKER_TIMEOUT_SECONDS
    || numericValues.job_poll_seconds < 0.01
    || numericValues.job_poll_seconds > MAX_JOB_POLL_SECONDS
    || numericValues.job_lease_seconds > MAX_JOB_LEASE_SECONDS
    || requiresKeyDecision
    || reasoningInvalid;
  const authState = configuration.codex_api_key_configured
    ? 'API KEY CONFIGURED'
    : configuration.codex_login_configured
      ? 'CODEX LOGIN CONFIGURED'
      : 'AUTH REQUIRED';

  const save = () => {
    const payload: RuntimeConfigurationUpdateWithCodexDefaults = {
      expected_revision: configuration.revision,
      codex_model: model.trim() || null,
      codex_reasoning_effort: reasoningEffort,
      codex_fast_mode: fastMode,
      codex_use_default_model_settings: useCodexDefaults,
      codex_base_url: baseUrl.trim() || null,
      codex_api_key: clearApiKey ? null : apiKey.trim() || null,
      clear_codex_api_key: clearApiKey,
      ...numericValues,
    };
    update.mutate(payload, {
      onSuccess: () => {
        setApiKey('');
        setClearApiKey(false);
      },
    });
  };

  return (
    <Section title="Runtime configuration" meta="Persisted operator settings; changes apply to newly claimed work without editing .env">
      <div className="qz-panel qz-panel-pad">
        <div className="qz-grid-4" style={{ marginBottom: 18 }}>
          <div><div className="qz-label">{t('runtime.codexAuth')}</div><div style={{ marginTop: 6 }}><StateBadge state={authState} /></div></div>
          <div><div className="qz-label">{t('runtime.providerEndpoint')}</div><div style={{ marginTop: 6 }}><StateBadge state={configuration.codex_base_url ? 'CUSTOM' : 'DEFAULT'} /></div></div>
          <div><div className="qz-label">{t('runtime.model')}</div><div className="qz-mono" style={{ marginTop: 8 }}>{useCodexDefaults || !configuration.codex_model ? t('runtime.codexDefault') : <bdi dir="ltr">{configuration.codex_model}</bdi>}</div></div>
          <div><div className="qz-label">{t('runtime.revision')}</div><div className="qz-number" style={{ marginTop: 8 }}>{formatNumber(configuration.revision)}</div></div>
        </div>

        <div className="qz-resource-note" style={{ marginBottom: 16 }}>
          {t('runtime.note')}
        </div>

        <ChatgptAuthControls configuration={configuration} />

        <div className="qz-form-grid">
          <div className="qz-field">
            <span className="qz-label" id="runtime-codex-defaults-label">{t('runtime.useDefault')}</span>
            <div style={{ display: 'flex', gap: 10, alignItems: 'center', minHeight: 32 }}>
              <Switch
                aria-labelledby="runtime-codex-defaults-label"
                checked={useCodexDefaults}
                disabled={update.isPending}
                onCheckedChange={setUseCodexDefaults}
              />
              <span className="qz-list-subtitle">
                <bdi dir="ltr">{useCodexDefaults ? t('runtime.codexDefault') : t('runtime.codexModel')}</bdi>
              </span>
            </div>
            <span className="qz-help">
              {useCodexDefaults ? t('runtime.reasoningDefault') : t('runtime.reasoningModelDependent')}
            </span>
          </div>
          <label className="qz-field">
            <span className="qz-label">{t('runtime.codexModel')}</span>
            <TextField.Root
              dir="ltr"
              value={model}
              disabled={update.isPending || useCodexDefaults}
              onChange={(event) => setModel(event.target.value)}
              placeholder={t('runtime.useDefault')}
            />
          </label>
          <div className="qz-field">
            <div className="qz-reasoning-header">
              <span className="qz-label" id="runtime-reasoning-label">{t('runtime.reasoningEffort')}</span>
              <span className="qz-reasoning-current" aria-live="polite"><bdi dir="ltr">{reasoningLabel}</bdi></span>
            </div>
            <Slider
              ref={reasoningSliderRef}
              aria-label={t('runtime.reasoningEffort')}
              aria-labelledby="runtime-reasoning-label"
              aria-valuetext={reasoningLabel}
              min={0}
              max={REASONING_EFFORT_STEPS.length - 1}
              step={1}
              value={[Math.max(0, selectedReasoningIndex)]}
              disabled={update.isPending || useCodexDefaults || reasoningInvalid}
              onValueChange={([nextIndex]) => {
                if (nextIndex !== undefined) setReasoningEffort(REASONING_EFFORT_STEPS[nextIndex].value);
              }}
            />
            <div className="qz-reasoning-marks" aria-hidden="true">
              {REASONING_EFFORT_STEPS.map((step, index) => (
                <span className="qz-reasoning-mark" key={step.key} data-selected={index === selectedReasoningIndex}>
                  <bdi dir="ltr">{t(step.key)}</bdi>
                </span>
              ))}
            </div>
            <span className="qz-help">{t('runtime.reasoningHelp')}</span>
            <span className="qz-list-subtitle">{t('runtime.reasoningModelDependent')}</span>
          </div>
          <div className="qz-field">
            <span className="qz-label" id="runtime-fast-label">{t('runtime.fastMode')}</span>
            <div style={{ display: 'flex', gap: 10, alignItems: 'center', minHeight: 32 }}>
              <Switch
                aria-labelledby="runtime-fast-label"
                checked={fastMode}
                disabled={update.isPending || useCodexDefaults}
                onCheckedChange={setFastMode}
              />
              <span className="qz-list-subtitle"><bdi dir="ltr">{fastMode ? t('runtime.fastEnabled') : t('runtime.fastStandard')}</bdi></span>
            </div>
            <span className="qz-help">{t('runtime.fastHelp')}</span>
          </div>
          <label className="qz-field">
            <span className="qz-label">{t('runtime.baseUrl')}</span>
            <TextField.Root dir="ltr" value={baseUrl} onChange={(event) => setBaseUrl(event.target.value)} placeholder="https://api.openai.com/v1" />
            <span className="qz-list-subtitle">{t('runtime.baseUrlHelp')}</span>
          </label>
          <label className="qz-field">
            <span className="qz-label">{t('runtime.apiKey')}</span>
            <TextField.Root dir="ltr" type="password" value={apiKey} disabled={clearApiKey} onChange={(event) => setApiKey(event.target.value)} placeholder={configuration.codex_api_key_configured ? t('runtime.keyConfigured') : t('runtime.keyOptional')} />
            {requiresKeyDecision ? <span className="qz-list-subtitle">{t('runtime.keyEndpointWarning')}</span> : null}
          </label>
          <label className="qz-field" style={{ alignSelf: 'end' }}>
            <span className="qz-label">{t('runtime.clearKey')}</span>
            <div style={{ display: 'flex', gap: 10, alignItems: 'center', minHeight: 32 }}>
              <Switch checked={clearApiKey} onCheckedChange={(checked) => { setClearApiKey(checked); if (checked) setApiKey(''); }} />
              <span className="qz-list-subtitle">{clearApiKey ? t('runtime.removeOnSave') : t('runtime.keepKey')}</span>
            </div>
          </label>
        </div>

        <div style={{ margin: '22px 0 10px' }} className="qz-label">{t('runtime.workerLimits')}</div>
        <div className="qz-form-grid">
          <label className="qz-field"><span className="qz-label">{t('runtime.maxWheel')}</span><TextField.Root dir="ltr" type="number" min="1" max={String(MAX_PLUGIN_WHEEL_BYTES)} step="1" value={maxWheelBytes} onChange={(event) => setMaxWheelBytes(event.target.value)} /><span className="qz-list-subtitle">{t('runtime.max1Gib', { max: MAX_PLUGIN_WHEEL_BYTES / 1024 ** 3 })}</span></label>
          <label className="qz-field"><span className="qz-label">{t('runtime.validationTimeout')}</span><TextField.Root dir="ltr" type="number" min="1" max={String(MAX_WORKER_TIMEOUT_SECONDS)} step="1" value={pluginValidationTimeout} onChange={(event) => setPluginValidationTimeout(event.target.value)} /></label>
          <label className="qz-field"><span className="qz-label">{t('runtime.bundleTimeout')}</span><TextField.Root dir="ltr" type="number" min="1" max={String(MAX_WORKER_TIMEOUT_SECONDS)} step="1" value={bundleBuildTimeout} onChange={(event) => setBundleBuildTimeout(event.target.value)} /></label>
          <label className="qz-field"><span className="qz-label">{t('runtime.pluginJobTimeout')}</span><TextField.Root dir="ltr" type="number" min="1" max={String(MAX_WORKER_TIMEOUT_SECONDS)} step="1" value={pluginJobTimeout} onChange={(event) => setPluginJobTimeout(event.target.value)} /></label>
          <label className="qz-field"><span className="qz-label">{t('runtime.missionTimeout')}</span><TextField.Root dir="ltr" type="number" min="1" max={String(MAX_WORKER_TIMEOUT_SECONDS)} step="1" value={missionJobTimeout} onChange={(event) => setMissionJobTimeout(event.target.value)} /><span className="qz-list-subtitle">{t('runtime.timeoutRange', { min: 1, max: MAX_WORKER_TIMEOUT_SECONDS })}</span></label>
          <label className="qz-field"><span className="qz-label">{t('runtime.pollInterval')}</span><TextField.Root dir="ltr" type="number" min="0.01" max={String(MAX_JOB_POLL_SECONDS)} step="0.01" value={jobPollSeconds} onChange={(event) => setJobPollSeconds(event.target.value)} /><span className="qz-list-subtitle">{t('runtime.pollRange', { min: 0.01, max: MAX_JOB_POLL_SECONDS })}</span></label>
          <label className="qz-field"><span className="qz-label">{t('runtime.jobLease')}</span><TextField.Root dir="ltr" type="number" min="1" max={String(MAX_JOB_LEASE_SECONDS)} step="1" value={jobLeaseSeconds} onChange={(event) => setJobLeaseSeconds(event.target.value)} /></label>
        </div>

        {update.error ? <div style={{ marginTop: 16 }}><ErrorPanel error={update.error} /></div> : null}
        <div style={{ display: 'flex', justifyContent: 'flex-end', marginTop: 18 }}>
          <Button disabled={invalid || update.isPending} onClick={save}>{update.isPending ? t('common.saving') : t('runtime.save')}</Button>
        </div>
      </div>
    </Section>
  );
}
