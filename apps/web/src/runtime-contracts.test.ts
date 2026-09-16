import { readFileSync } from 'node:fs';
import Ajv2020 from 'ajv/dist/2020.js';
import addFormats from 'ajv-formats';
import { describe, expect, it } from 'vitest';

const document = JSON.parse(readFileSync(new URL('../../../contracts/generated/api-v2.openapi.json', import.meta.url), 'utf8'));
const root = 'urn:quazonai:runtime-contract-boundaries';
const ajv = new Ajv2020({ strict: false, validateFormats: true });
addFormats(ajv);
ajv.addSchema(document, root);
function accepts(name: string, value: unknown): boolean {
  const validate = ajv.getSchema(`${root}#/components/schemas/${name}`);
  if (!validate) throw new Error(`Missing native generated schema: ${name}`);
  return !!validate(value);
}
const id = '01991f2a-0000-7000-8000-000000000001';
function runtime() {
  return { schema_version: 1, configuration: {
    name: 'Native Runtime', endpoint: 'https://runtime.example', tls_policy: 'SYSTEM_CA',
    allowed_capabilities: ['DATA_VALIDATE'], enabled: true, development_http: false,
  }, credential_ref: id, ca_certificate_ref: null as string | null };
}
function secret(purpose: string, value: string) {
  return { intent: { schema_version: 1, purpose, label: 'Integration fixture' }, value };
}

describe('native purpose-dependent secret request schema', () => {
  it('bounds Runtime credentials before a transport can misclassify short reflection matches', () => {
    for (const length of [0, 1, 31, 8193]) {
      expect(accepts('IntegrationSecretCreate', secret('RUNTIME', 'x'.repeat(length)))).toBe(false);
    }
    for (const length of [32, 8192]) {
      expect(accepts('IntegrationSecretCreate', secret('RUNTIME', 'x'.repeat(length)))).toBe(true);
    }
    for (const suffix of [' ', '\n', '\t', '\0', '中']) {
      expect(accepts('IntegrationSecretCreate', secret('RUNTIME', 'x'.repeat(32) + suffix))).toBe(false);
    }
  });
  it('preserves upstream-compatible Downstream and Custom Provider credential bounds', () => {
    for (const purpose of ['DOWNSTREAM', 'CUSTOM_PROVIDER']) {
      for (const length of [1, 8192]) {
        expect(accepts('IntegrationSecretCreate', secret(purpose, 'x'.repeat(length)))).toBe(true);
      }
      for (const value of ['', 'x'.repeat(8193), 'a b', 'a\n', 'a\0', '中文']) {
        expect(accepts('IntegrationSecretCreate', secret(purpose, value))).toBe(false);
      }
    }
  });
  it('bounds ASCII CA material without pretending JSON Schema can verify a certificate', () => {
    for (const value of ['a\nb\n', 'x'.repeat(65536)]) {
      expect(accepts('IntegrationSecretCreate', secret('TLS_CA', value))).toBe(true);
    }
    for (const value of ['', 'x'.repeat(65537), '中文']) {
      expect(accepts('IntegrationSecretCreate', secret('TLS_CA', value))).toBe(false);
    }
    expect(accepts('IntegrationSecretCreate', secret('UNKNOWN', 'x'.repeat(32)))).toBe(false);
    expect(accepts('IntegrationSecretCreate', { ...secret('RUNTIME', 'x'.repeat(32)), admin: true })).toBe(false);
  });
});

describe('native TLS and CA reference dependencies', () => {
  it('requires a CA for pinned creation and forbids unused CA references for system roots', () => {
    const request = runtime();
    expect(accepts('RuntimeCreate', request)).toBe(true);
    request.ca_certificate_ref = id;
    expect(accepts('RuntimeCreate', request)).toBe(false);
    request.configuration.tls_policy = 'PINNED_CA';
    expect(accepts('RuntimeCreate', request)).toBe(true);
    request.ca_certificate_ref = null;
    expect(accepts('RuntimeCreate', request)).toBe(false);
    Reflect.deleteProperty(request, 'ca_certificate_ref');
    expect(accepts('RuntimeCreate', request)).toBe(false);
    request.ca_certificate_ref = id;
    request.configuration.development_http = true;
    expect(accepts('RuntimeCreate', request)).toBe(false);
  });
  it('keeps pinned update CA retention distinct from fresh creation', () => {
    const request = { ...runtime(), expected_revision: '1', credential_ref: null };
    request.configuration.tls_policy = 'PINNED_CA';
    expect(accepts('RuntimeUpdate', request)).toBe(true);
    Reflect.deleteProperty(request, 'ca_certificate_ref');
    expect(accepts('RuntimeUpdate', request)).toBe(true);
    request.ca_certificate_ref = id;
    expect(accepts('RuntimeUpdate', request)).toBe(true);
    request.configuration.tls_policy = 'SYSTEM_CA';
    expect(accepts('RuntimeUpdate', request)).toBe(false);
    request.ca_certificate_ref = null;
    expect(accepts('RuntimeUpdate', request)).toBe(true);
    request.configuration.endpoint = 'http://127.0.0.1:8090';
    request.configuration.development_http = true;
    expect(accepts('RuntimeUpdate', request)).toBe(true);
    request.configuration.tls_policy = 'PINNED_CA';
    expect(accepts('RuntimeUpdate', request)).toBe(false);
  });
});

describe('native Runtime engine-version map bounds', () => {
  it('uses the same native map contract for probes and immutable result manifests', () => {
    const domain = JSON.parse(readFileSync(new URL('../../../contracts/generated/domain-v1.openapi.json', import.meta.url), 'utf8'));
    const schemas = domain.components.schemas;
    expect(schemas.ResultManifestV1.properties.engine_versions)
      .toEqual(schemas.RuntimeCapabilitiesV1.properties.engine_versions);
    expect(schemas.RuntimeCapabilitiesV1.properties.engine_versions)
      .toEqual(document.components.schemas.RuntimeCapabilitiesV1.properties.engine_versions);
  });
  function capabilities() {
    return JSON.parse(readFileSync(new URL('../../../tests/contracts/runtime-capabilities.fixture.json', import.meta.url), 'utf8')) as { engine_versions: Record<string, string> };
  }
  it('requires between one and 64 named versions', () => {
    const request = capabilities();
    for (const count of [0, 1, 64, 65]) {
      request.engine_versions = Object.fromEntries(Array.from({ length: count }, (_, i) => [`engine-${i}`, '1']));
      expect(accepts('RuntimeCapabilitiesV1', request)).toBe(count >= 1 && count <= 64);
    }
  });
  it('validates map keys and values, including non-BMP character counts and control characters', () => {
    const request = capabilities();
    for (const text of ['a', 'a'.repeat(120), '😀'.repeat(120), '\uFEFF']) {
      request.engine_versions = { [text]: text };
      expect(accepts('RuntimeCapabilitiesV1', request)).toBe(true);
    }
    for (const text of ['', 'a'.repeat(121), '😀'.repeat(121), ' ', '\t', '\n', 'a\0', 'a\u0085', '\u3000']) {
      request.engine_versions = { [text]: '1' };
      expect(accepts('RuntimeCapabilitiesV1', request)).toBe(false);
      request.engine_versions = { engine: text };
      expect(accepts('RuntimeCapabilitiesV1', request)).toBe(false);
    }
  });
});
