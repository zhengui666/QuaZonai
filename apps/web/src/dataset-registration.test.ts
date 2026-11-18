import { createElement, useRef, type ReactNode } from 'react';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import i18n from './i18n';

// Only replace Ant Design's DOM form shell. The component's submit handler,
// React state, query invalidation and translations remain real.
type Values = Record<string, unknown>;
type FormHandle = { getFieldValue: (name: string) => unknown; setFieldValue: (name: string, value: unknown) => void; getFieldsValue: () => Values };
vi.mock('antd', () => {
  const Form = Object.assign(({ form, initialValues, children, ...props }: { form: FormHandle; initialValues: Values; children: ReactNode }) => {
    const initialized = useRef(false);
    if (!initialized.current) {
      for (const [name, value] of Object.entries(initialValues)) form.setFieldValue(name, value);
      initialized.current = true;
    }
    return createElement('form', { ...props, form }, children);
  }, {
    useForm: () => {
      const ref = useRef<FormHandle | null>(null);
      if (!ref.current) {
        const values: Values = {};
        ref.current = { getFieldValue: name => values[name], setFieldValue: (name, value) => { values[name] = value; }, getFieldsValue: () => ({ ...values }) };
      }
      return [ref.current];
    },
    Item: ({ children, label }: { children: ReactNode; label: ReactNode }) => createElement('label', null, label, children),
  });
  return { Form, Input: () => createElement('input'), InputNumber: () => createElement('input'), Button: (props: Record<string, unknown>) => createElement('button', props) };
});
vi.mock('./api', () => ({ api: vi.fn() }));
vi.mock('./data-hooks', () => ({ useResource: vi.fn() }));
vi.mock('./ui', () => ({ ErrorNotice: ({ error }: { error?: unknown }) => error ? createElement('p', { role: 'alert' }, String(error)) : null }));
import { api } from './api';
import { useResource } from './data-hooks';
import DatasetRegistration from './dataset-registration';

let renderer: ReactTestRenderer | undefined;
let cache: QueryClient;
beforeEach(async () => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.mocked(api).mockReset();
  vi.mocked(useResource).mockReturnValue({ data: { revision: 7, datasetIds: ['existing'] }, error: null, refetch: vi.fn() } as unknown as ReturnType<typeof useResource>);
  cache = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  await i18n.changeLanguage('en');
});
afterEach(async () => {
  if (renderer) await act(async () => { renderer!.unmount(); });
  renderer = undefined; cache.clear(); vi.unstubAllGlobals();
  await i18n.changeLanguage('en');
});
async function mount() {
  await act(async () => { renderer = create(createElement(QueryClientProvider, { client: cache }, createElement(DatasetRegistration))); });
}
const form = () => renderer!.root.findByType('form');
const fields = () => form().props.form as FormHandle;
async function submit() { await act(async () => { form().props.onFinish(fields().getFieldsValue()); }); }

it('submits the exact descriptor request and displays only the server receipt', async () => {
  const invalidate = vi.spyOn(cache, 'invalidateQueries');
  const receipt = { requestId: 'original-registration', revision: 8, datasetIds: ['native-btc', 'native-eth'] };
  vi.mocked(api).mockResolvedValue(receipt);
  await mount();
  expect(useResource).toHaveBeenCalledWith('/datasets/registry');
  expect(fields().getFieldValue('expectedRevision')).toBe(7);
  fields().setFieldValue('requestId', receipt.requestId);
  fields().setFieldValue('path', '/server/native/catalog-descriptor.json');
  await submit();
  expect(api).toHaveBeenCalledTimes(1);
  expect(api).toHaveBeenCalledWith('/datasets/register', { method: 'POST', body: { requestId: receipt.requestId, expectedRevision: 7, path: '/server/native/catalog-descriptor.json' } });
  expect(renderer!.root.findByProps({ role: 'status' }).findAllByType('code').map(node => node.children.join(''))).toEqual(['native-btc, native-eth', receipt.requestId]);
  expect(renderer!.root.findByProps({ role: 'status' }).children).toContain('8');
  expect(invalidate).toHaveBeenCalledTimes(1);
  expect(renderer!.root.findByType('button').props.loading).toBe(false);
});

it('keeps the original ID, revision and path after an uncertain failure and retries unchanged', async () => {
  const invalidate = vi.spyOn(cache, 'invalidateQueries');
  vi.mocked(api).mockRejectedValueOnce(new Error('connection_lost'));
  await mount();
  const originalId = fields().getFieldValue('requestId');
  expect(typeof originalId).toBe('string');
  fields().setFieldValue('path', '/server/original.json');
  const original = fields().getFieldsValue();
  await submit();
  expect(fields().getFieldsValue()).toEqual(original);
  expect(renderer!.root.findByProps({ role: 'alert' }).children.join('')).toContain('connection_lost');
  expect(renderer!.root.findAllByProps({ role: 'status' })).toHaveLength(0);
  expect(invalidate).not.toHaveBeenCalled();
  expect(form().props.disabled).toBe(false);
  vi.mocked(api).mockResolvedValueOnce({ requestId: originalId, revision: 8, datasetIds: ['original-dataset'] });
  await submit();
  expect(vi.mocked(api).mock.calls).toEqual([
    ['/datasets/register', { method: 'POST', body: original }],
    ['/datasets/register', { method: 'POST', body: original }],
  ]);
  expect(renderer!.root.findAllByProps({ role: 'alert' })).toHaveLength(0);
  expect(renderer!.root.findByProps({ role: 'status' }).findAllByType('code')[0]!.children.join('')).toBe('original-dataset');
});
