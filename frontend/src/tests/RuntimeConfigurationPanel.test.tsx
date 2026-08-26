import { screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { RuntimeConfigurationPanel } from '../components/admin/RuntimeConfigurationPanel';
import { translateKey } from '../i18n';
import type { RuntimeConfiguration } from '../lib/api/types';
import { renderApp } from './testUtils';

const configuration: RuntimeConfiguration = {
  revision: 1,
  codex_model: 'gpt-5.6-sol',
  codex_base_url: 'https://gateway.example/v1',
  codex_api_key_configured: true,
  codex_login_configured: false,
  max_plugin_wheel_bytes: 1_073_741_824,
  plugin_validation_timeout_seconds: 900,
  bundle_build_timeout_seconds: 900,
  plugin_job_timeout_seconds: 900,
  mission_job_timeout_seconds: 900,
  job_poll_seconds: 1,
  job_lease_seconds: 900,
};

describe('RuntimeConfigurationPanel directionality', () => {
  it('keeps machine identifier inputs left-to-right in Arabic', () => {
    renderApp(<RuntimeConfigurationPanel configuration={configuration} />, { locale: 'ar' });
    expect(screen.getByDisplayValue('gpt-5.6-sol')).toHaveAttribute('dir', 'ltr');
    expect(screen.getByDisplayValue('https://gateway.example/v1')).toHaveAttribute('dir', 'ltr');
    expect(screen.getByPlaceholderText(translateKey('ar', 'runtime.keyConfigured'))).toHaveAttribute('dir', 'ltr');
  });
});
