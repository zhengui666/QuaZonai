import { Button, Switch, TextField } from '@radix-ui/themes';
import { useEffect, useMemo, useState } from 'react';
import { useI18n } from '../../i18n';
import { useUpdateRuntimeConfiguration } from '../../lib/api/hooks';
import type { RuntimeConfiguration } from '../../lib/api/types';
import { ErrorPanel } from '../ui/ErrorPanel';
import { Section } from '../ui/Section';
import { StateBadge } from '../ui/StateBadge';

const MAX_PLUGIN_WHEEL_BYTES = 1_073_741_824;
const MAX_WORKER_TIMEOUT_SECONDS = 86_400;
const MAX_JOB_POLL_SECONDS = 3600;
const MAX_JOB_LEASE_SECONDS = 86_400;

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

export function RuntimeConfigurationPanel({ configuration }: { configuration: RuntimeConfiguration }) {
  const { t } = useI18n();
  const update = useUpdateRuntimeConfiguration();
  const [model, setModel] = useState(configuration.codex_model ?? '');
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
    setModel(configuration.codex_model ?? '');
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
    || requiresKeyDecision;
  const authState = configuration.codex_api_key_configured
    ? 'API KEY CONFIGURED'
    : configuration.codex_login_configured
      ? 'CODEX LOGIN CONFIGURED'
      : 'AUTH REQUIRED';

  const save = () => {
    update.mutate({
      expected_revision: configuration.revision,
      codex_model: model.trim() || null,
      codex_base_url: baseUrl.trim() || null,
      codex_api_key: clearApiKey ? null : apiKey.trim() || null,
      clear_codex_api_key: clearApiKey,
      ...numericValues,
    }, {
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
          <div><div className="qz-label">{t('runtime.model')}</div><div className="qz-mono" style={{ marginTop: 8 }}>{configuration.codex_model || t('runtime.codexDefault')}</div></div>
          <div><div className="qz-label">{t('runtime.revision')}</div><div className="qz-number" style={{ marginTop: 8 }}>{configuration.revision}</div></div>
        </div>

        <div className="qz-resource-note" style={{ marginBottom: 16 }}>
          {t('runtime.note')}
        </div>

        <div className="qz-form-grid">
          <label className="qz-field">
            <span className="qz-label">{t('runtime.codexModel')}</span>
            <TextField.Root value={model} onChange={(event) => setModel(event.target.value)} placeholder={t('runtime.useDefault')} />
          </label>
          <label className="qz-field">
            <span className="qz-label">{t('runtime.baseUrl')}</span>
            <TextField.Root value={baseUrl} onChange={(event) => setBaseUrl(event.target.value)} placeholder="https://api.openai.com/v1" />
            <span className="qz-list-subtitle">{t('runtime.baseUrlHelp')}</span>
          </label>
          <label className="qz-field">
            <span className="qz-label">{t('runtime.apiKey')}</span>
            <TextField.Root type="password" value={apiKey} disabled={clearApiKey} onChange={(event) => setApiKey(event.target.value)} placeholder={configuration.codex_api_key_configured ? t('runtime.keyConfigured') : t('runtime.keyOptional')} />
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
          <label className="qz-field"><span className="qz-label">{t('runtime.maxWheel')}</span><TextField.Root type="number" min="1" max={String(MAX_PLUGIN_WHEEL_BYTES)} step="1" value={maxWheelBytes} onChange={(event) => setMaxWheelBytes(event.target.value)} /><span className="qz-list-subtitle">{t('runtime.max1Gib')}</span></label>
          <label className="qz-field"><span className="qz-label">{t('runtime.validationTimeout')}</span><TextField.Root type="number" min="1" max={String(MAX_WORKER_TIMEOUT_SECONDS)} step="1" value={pluginValidationTimeout} onChange={(event) => setPluginValidationTimeout(event.target.value)} /></label>
          <label className="qz-field"><span className="qz-label">{t('runtime.bundleTimeout')}</span><TextField.Root type="number" min="1" max={String(MAX_WORKER_TIMEOUT_SECONDS)} step="1" value={bundleBuildTimeout} onChange={(event) => setBundleBuildTimeout(event.target.value)} /></label>
          <label className="qz-field"><span className="qz-label">{t('runtime.pluginJobTimeout')}</span><TextField.Root type="number" min="1" max={String(MAX_WORKER_TIMEOUT_SECONDS)} step="1" value={pluginJobTimeout} onChange={(event) => setPluginJobTimeout(event.target.value)} /></label>
          <label className="qz-field"><span className="qz-label">{t('runtime.missionTimeout')}</span><TextField.Root type="number" min="1" max={String(MAX_WORKER_TIMEOUT_SECONDS)} step="1" value={missionJobTimeout} onChange={(event) => setMissionJobTimeout(event.target.value)} /><span className="qz-list-subtitle">{t('runtime.timeoutRange')}</span></label>
          <label className="qz-field"><span className="qz-label">{t('runtime.pollInterval')}</span><TextField.Root type="number" min="0.01" max={String(MAX_JOB_POLL_SECONDS)} step="0.01" value={jobPollSeconds} onChange={(event) => setJobPollSeconds(event.target.value)} /><span className="qz-list-subtitle">{t('runtime.pollRange')}</span></label>
          <label className="qz-field"><span className="qz-label">{t('runtime.jobLease')}</span><TextField.Root type="number" min="1" max={String(MAX_JOB_LEASE_SECONDS)} step="1" value={jobLeaseSeconds} onChange={(event) => setJobLeaseSeconds(event.target.value)} /></label>
        </div>

        {update.error ? <div style={{ marginTop: 16 }}><ErrorPanel error={update.error} /></div> : null}
        <div style={{ display: 'flex', justifyContent: 'flex-end', marginTop: 18 }}>
          <Button disabled={invalid || update.isPending} onClick={save}>{update.isPending ? t('common.saving') : t('runtime.save')}</Button>
        </div>
      </div>
    </Section>
  );
}
