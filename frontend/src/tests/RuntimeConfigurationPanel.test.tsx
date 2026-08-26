import { screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { RuntimeConfigurationPanel } from '../components/admin/RuntimeConfigurationPanel';
import { translateKey } from '../i18n';
import type { RuntimeConfiguration } from '../lib/api/types';
import { renderApp } from './testUtils';

const configuration: RuntimeConfiguration = {
  revision: 1234,
  codex_model: 'openai/gpt-5.6-sol',
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
  it('keeps machine identifiers left-to-right and formats the revision in Arabic', () => {
    renderApp(<RuntimeConfigurationPanel configuration={configuration} />, { locale: 'ar' });
    expect(screen.getByDisplayValue('openai/gpt-5.6-sol')).toHaveAttribute('dir', 'ltr');
    expect(screen.getByText('openai/gpt-5.6-sol')).toHaveAttribute('dir', 'ltr');
    expect(screen.getByText(new Intl.NumberFormat('ar').format(configuration.revision))).toBeInTheDocument();
    expect(screen.getByDisplayValue('https://gateway.example/v1')).toHaveAttribute('dir', 'ltr');
    expect(screen.getByPlaceholderText(translateKey('ar', 'runtime.keyConfigured'))).toHaveAttribute('dir', 'ltr');
    expect(screen.getByText(translateKey('ar', 'runtime.max1Gib', { max: 1 }))).toBeInTheDocument();
    expect(screen.getByText(translateKey('ar', 'runtime.timeoutRange', { min: 1, max: 86_400 }))).toBeInTheDocument();
    expect(screen.getByText(translateKey('ar', 'runtime.pollRange', { min: 0.01, max: 3600 }))).toBeInTheDocument();
    const numericInputs = screen.getAllByRole('spinbutton');
    expect(numericInputs).toHaveLength(7);
    numericInputs.forEach((input) => expect(input).toHaveAttribute('dir', 'ltr'));
  });

  it('uses the locale formatter for decimal range bounds', () => {
    expect(translateKey('es', 'runtime.pollRange', { min: 0.01, max: 3600 })).toBe(`0,01–${new Intl.NumberFormat('es').format(3600)} segundos.`);
  });
});
