import { fireEvent, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { RuntimeConfigurationPanel } from '../components/admin/RuntimeConfigurationPanel';
import { translateKey } from '../i18n';
import type { RuntimeConfiguration } from '../lib/api/types';
import { jsonResponse, renderApp } from './testUtils';

const configuration: RuntimeConfiguration = {
  revision: 1234,
  codex_model: 'openai/gpt-5.6-sol',
  codex_reasoning_effort: 'high',
  codex_fast_mode: false,
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
    expect(screen.getByRole('slider')).toHaveAttribute('aria-valuetext', translateKey('ar', 'runtime.reasoningHigh'));
    expect(screen.getByRole('switch', { name: translateKey('ar', 'runtime.useDefault') })).toBeInTheDocument();
    expect(screen.getByRole('switch', { name: translateKey('ar', 'runtime.fastMode') })).toBeInTheDocument();
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

  it('sends semantic reasoning and Fast values instead of slider indexes', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation(() => jsonResponse(configuration));
    renderApp(<RuntimeConfigurationPanel configuration={configuration} />);

    fireEvent.keyDown(screen.getByRole('slider'), { key: 'End' });
    fireEvent.click(screen.getByRole('switch', { name: translateKey('en', 'runtime.fastMode') }));
    fireEvent.click(screen.getByRole('button', { name: translateKey('en', 'runtime.save') }));

    await waitFor(() => expect(fetchMock).toHaveBeenCalledWith(
      '/api/v1/system/runtime-configuration',
      expect.objectContaining({ method: 'PUT' }),
    ));
    const options = fetchMock.mock.calls[0][1] as RequestInit;
    expect(JSON.parse(String(options.body))).toMatchObject({
      codex_reasoning_effort: 'xhigh',
      codex_fast_mode: true,
      codex_use_default_model_settings: false,
    });
    expect(JSON.parse(String(options.body))).not.toHaveProperty('reasoning_effort_index');
  });

  it('uses Codex model defaults without clearing retained QuaZonai overrides', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation(() => jsonResponse({
      ...configuration,
      codex_use_default_model_settings: true,
    }));
    renderApp(<RuntimeConfigurationPanel configuration={{
      ...configuration,
      codex_use_default_model_settings: false,
    }} />);

    const defaultsSwitch = screen.getByRole('switch', {
      name: translateKey('en', 'runtime.useDefault'),
    });
    const modelInput = screen.getByDisplayValue('openai/gpt-5.6-sol');
    const reasoningSlider = screen.getByRole('slider');
    const fastSwitch = screen.getByRole('switch', {
      name: translateKey('en', 'runtime.fastMode'),
    });
    const providerInput = screen.getByDisplayValue('https://gateway.example/v1');

    expect(modelInput).toBeEnabled();
    expect(reasoningSlider).toBeEnabled();
    expect(fastSwitch).toBeEnabled();
    fireEvent.click(defaultsSwitch);
    expect(modelInput).toBeDisabled();
    expect(reasoningSlider).toHaveAttribute('data-disabled');
    expect(fastSwitch).toBeDisabled();
    expect(providerInput).toBeEnabled();

    fireEvent.click(screen.getByRole('button', { name: translateKey('en', 'runtime.save') }));
    await waitFor(() => expect(fetchMock).toHaveBeenCalled());
    const options = fetchMock.mock.calls[0][1] as RequestInit;
    expect(JSON.parse(String(options.body))).toMatchObject({
      codex_use_default_model_settings: true,
      codex_model: 'openai/gpt-5.6-sol',
      codex_reasoning_effort: 'high',
      codex_fast_mode: false,
      codex_base_url: 'https://gateway.example/v1',
    });
  });
});
