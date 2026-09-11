import { describe, expect, it } from 'vitest';
import Ajv2020 from 'ajv/dist/2020.js';
import addFormats from 'ajv-formats';
import document from '../../../contracts/generated/api-v2.openapi.json';
import cases from '../../../tests/contracts/data-registry-keys.json';
import { validateNativeCatalogKey, validateResponse } from './generated/responses.cjs';

const ajv = new Ajv2020({ strict: false });
addFormats(ajv);
ajv.addSchema(document, 'urn:qz:data-registry-test');
const native = ajv.getSchema('urn:qz:data-registry-test#/components/schemas/DataSourceCreate/properties/native_catalog_ref');
if (!native) throw new Error('Native registry-key schema missing');

describe('native registry-key grammar', () => {
  it.each(cases)('$value matches the domain corpus', ({ value, valid }) => {
    expect(native(value)).toBe(valid);
    expect(validateNativeCatalogKey(value)).toBe(valid);
  });
  it('counts Unicode scalar characters, not UTF-16 units or UTF-8 bytes', () => {
    for (const unit of ['a', '数', '🧪']) {
      expect(validateNativeCatalogKey(unit.repeat(512))).toBe(true);
      expect(validateNativeCatalogKey(unit.repeat(513))).toBe(false);
    }
    expect(validateNativeCatalogKey(null)).toBe(false);
    expect(validateNativeCatalogKey({ value: 'catalog' })).toBe(false);
  });
  it('requires explicit Universe registration evidence state', () => {
    const value = { id: '0198dfff-0000-7000-8000-000000000001', name: 'Historical Universe',
      membership_artifact_id: '0198dfff-0000-7000-8000-000000000002',
      instrument_definitions_artifact_id: '0198dfff-0000-7000-8000-000000000003',
      calendar_ref: 'fixture', calendar_version: '1', selection_asof: '2020-01-01T00:00:00Z',
      has_historical_membership: false, coverage_start: '2020-01-01T00:00:00Z',
      coverage_end: '2020-01-02T00:00:00Z', created_at: '2020-01-03T00:00:00Z' };
    const path = '/api/v2/data/universes/{id}';
    expect(validateResponse(path, 'GET', 200, value)).toBe(false);
    for (const registration_state of ['NATIVE_METADATA', 'LEGACY_UNVERIFIED']) {
      expect(validateResponse(path, 'GET', 200, { ...value, registration_state })).toBe(true);
    }
    expect(validateResponse(path, 'GET', 200, { ...value, registration_state: 'QUALIFIED' })).toBe(false);
  });
});
