import { useTranslation } from 'react-i18next';
import i18n from './i18n';
const t = (key: string, values?: Record<string, unknown>): string => i18n.t(key, { ns: 'settings', ...values });
import {PreparationStatus,SelectedRunnerStatus,type PreparationView} from './native-preparation-status';
import { href } from './router';
import { Button, Checkbox, Form, Input, Select } from 'antd';
import { useQueryClient } from '@tanstack/react-query';
import { useEffect, useState } from 'react';
import type { InstanceSettingsView, NativeEnvironmentSettingsUpdate } from '../../../packages/contracts/src/index.ts';
import type { NativeRustEnvironment } from '../../../packages/contracts/src/nautilus.ts';
import { ApiError, api } from './api';
import { useResource } from './data-hooks';
import { ErrorNotice, Loading,useConfirm } from './ui';

type Fields = Omit<NativeRustEnvironment, 'kind' | 'features'> & { featureNames?: string };
export function environmentFromFields(values: Fields): NativeRustEnvironment {
  const accountFactory=values.accountFactory?.trim();
  if(values.accountCompatibility!==undefined && (values.accountCompatibility!=='TRUSTED_STRATEGY_ONLY_SINGLE_NODE_V1'||!accountFactory))throw new Error(t("A compatibility declaration requires an account strategy factory. Clear this option if you are not making a declaration."));
  if(values.backtestCompletion!==undefined && (typeof values.backtestCompletion!=='boolean'||values.backtestCompletion&&!values.backtestFactory?.trim()))throw new Error(t("The completion callback requires a backtest factory and an explicit boolean."));
  if(values.hfCatalogs!==undefined && typeof values.hfCatalogs!=='boolean')throw new Error(t("Choose whether this runner should read public Hugging Face catalogs."));
  return {kind:'RUST_PROCESS',id:values.id.trim(),cargoBinary:values.cargoBinary.trim(),manifestPath:values.manifestPath.trim(),packageName:values.packageName.trim(),
    features:(values.featureNames??'').split(',').map(v=>v.trim()).filter(Boolean),
    ...(values.backtestFactory?.trim()?{backtestFactory:values.backtestFactory.trim()}:{}),...(accountFactory?{accountFactory}:{}),
    ...(values.backtestCompletion!==undefined?{backtestCompletion:values.backtestCompletion}:{}),
    ...(values.hfCatalogs!==undefined?{hfCatalogs:values.hfCatalogs}:{}),
    ...(values.accountCompatibility?{accountCompatibility:values.accountCompatibility}:{}),
  };
}
export function AccountCompatibilityNotice({declaration}:{declaration:unknown}) {
 const { t } = useTranslation('settings');

  return <p>{declaration==='TRUSTED_STRATEGY_ONLY_SINGLE_NODE_V1'?t("Trusted single-node strategy: self-declared, not verified"):t("Trusted single-node strategy: not declared")}</p>;
}
export function retryUncertainEnvironmentSave(status?: number) { return status === undefined || status >= 500; }
type BuildRequest={requestId:string;expectedRevision:number;environmentId?:string;sourceExecutable?:string};
type BuildView=PreparationView;

export default function InstanceSettings() {
 const { t } = useTranslation('settings');
 const required = [{ required: true, whitespace: true, message: t('Fill in this field') }];

  const query = useResource<InstanceSettingsView>('/settings/instances');
  const cache = useQueryClient();
  const confirm=useConfirm();
  const [importId,setImportId]=useState(''),[importPath,setImportPath]=useState('');
  const [form] = Form.useForm<Fields>();
  const [loaded, setLoaded] = useState(false), [busy, setBusy] = useState(false);
  const [saved, setSaved] = useState<InstanceSettingsView>();
  const [pending, setPending] = useState<NativeEnvironmentSettingsUpdate>();
  const [error, setError] = useState<unknown>(); const [notice, setNotice] = useState('');
  const [discovery, setDiscovery] = useState<unknown>();
  const [buildRequest,setBuildRequest]=useState<BuildRequest|undefined>(()=>{try{return JSON.parse(localStorage.getItem('qz-native-build-request')??'null')??undefined;}catch{return undefined;}});
  const [build,setBuild]=useState<BuildView>();
  useEffect(() => {
    if (!query.data || loaded) return;
    setSaved(query.data); form.setFieldsValue({ ...query.data.environment, featureNames: query.data.environment?.features?.join(',') }); setLoaded(true);
  }, [query.data, loaded, form]);
  async function save() {
    if (!saved || busy) return;
    setError(undefined); setNotice('');
    try {
      const values = pending ? null : await form.validateFields();
      const request = pending ?? {expectedRevision:saved.revision,environment:environmentFromFields(values!)};
      setBusy(true); setPending(request);
      const result = await api<InstanceSettingsView>('/settings/instances',{method:'PUT',body:request});
      setSaved(result); void cache.invalidateQueries({ queryKey: ['/settings'] }); setPending(undefined); setDiscovery(undefined); setNotice(t("Configuration saved. Select an artifact again. No account was connected."));
    } catch (failure) {
      setError(failure);
      if (!retryUncertainEnvironmentSave(failure instanceof ApiError ? failure.status : undefined)) {
        setPending(undefined);
        if (failure instanceof ApiError && failure.status === 409) {
          try { setSaved(await api<InstanceSettingsView>('/settings/instances')); } catch { /* Keep original revision until explicit refresh succeeds. */ }
        }
      }
    } finally { setBusy(false); }
  }
  async function inspect() {
    if (busy || pending) return; setBusy(true); setError(undefined); setDiscovery(undefined);
    try { setDiscovery(await api('/settings/instances/inspect',{method:'POST',body:{}})); }
    catch (failure) { setError(failure); } finally { setBusy(false); }
  }
  async function buildEnvironment(){
    if(busy||pending||!saved||(!saved.environment&&!buildRequest))return;setBusy(true);setError(undefined);
    const request=buildRequest??{requestId:crypto.randomUUID(),expectedRevision:saved.revision};
    setBuildRequest(request);localStorage.setItem('qz-native-build-request',JSON.stringify(request));
    try{setBuild(await api<BuildView>(request.sourceExecutable?'/settings/instances/import':'/settings/instances/build',{method:'POST',body:request}));}
    catch(failure){setError(failure);}finally{setBusy(false);}
  }
  async function importEnvironment(){
    if(busy||pending||!saved)return;
    const request:BuildRequest=buildRequest??{requestId:crypto.randomUUID(),expectedRevision:saved.revision,environmentId:saved.environment?.id??importId,sourceExecutable:importPath};
    if(!request.sourceExecutable){setError(new Error(t("Check or finish the original build request before preparing an import.")));return;}
    setBusy(true);setError(undefined);setBuildRequest(request);localStorage.setItem('qz-native-build-request',JSON.stringify(request));
    try{setBuild(await api<BuildView>('/settings/instances/import',{method:'POST',body:request}));}catch(failure){setError(failure);}finally{setBusy(false);}
  }
  async function readBuild(){if(!buildRequest||busy)return;setBusy(true);setError(undefined);try{setBuild(await api<BuildView>(`/settings/instances/build/${encodeURIComponent(buildRequest.requestId)}`));}catch(failure){setError(failure);}finally{setBusy(false);}}
  async function selectRunner(){if(!buildRequest||!saved||busy)return;setBusy(true);setError(undefined);try{setSaved(await api<InstanceSettingsView>('/settings/instances/select-runner',{method:'POST',body:{requestId:buildRequest.requestId,expectedRevision:buildRequest.expectedRevision}}));void cache.invalidateQueries({ queryKey: ['/settings'] });setNotice(t("Artifact selected. No account was connected."));}catch(failure){setError(failure);}finally{setBusy(false);}}
  if (!loaded) return query.error ? <ErrorNotice error={query.error} /> : <Loading />;
  return <div className="settings-page instance-settings"><header className="page-heading"><h2>{t("Nautilus native Rust environment")}</h2></header>

    <p>{t("Paths must be accessible from the application runtime.")}</p>

    {!!saved && (!!saved.backtest || saved.connections.length > 0) && <p>{t("Legacy HTTP configuration is retained.")}</p>}
    <Form form={form} layout="vertical" disabled={busy || !!pending}>
      <Form.Item name="id" label={t("Environment ID")} rules={required}><Input /></Form.Item>
      <Form.Item name="cargoBinary" label={t("Absolute path to existing Cargo executable")} rules={required}><Input /></Form.Item>
      <Form.Item name="manifestPath" label={t("Absolute path to existing Cargo.toml")} rules={required}><Input /></Form.Item>
      <Form.Item name="packageName" label={t("Strategy library package name")} rules={required}><Input /></Form.Item>
      <Form.Item name="featureNames" label={t("Explicit features (comma-separated)")}><Input /></Form.Item>
      <Form.Item name="backtestFactory" label={t("Public backtest factory module (config / install)")}><Input /></Form.Item>
      <Form.Item name="backtestCompletion" valuePropName="checked" label={t("Completion check after the native run returns")} extra={t("Explicitly enable finalize_after_node_run in the same module. Requires a rebuild; stopping is not natural EOF.")}><Checkbox>{t("Enable")}</Checkbox></Form.Item>
      <Form.Item name="hfCatalogs" valuePropName="checked" extra={t("Applies to the next build. Build and select that runner to use a pinned cloud catalog.")}><Checkbox>{t("Read public Hugging Face catalogs")}</Checkbox></Form.Item>
      <Form.Item name="accountFactory" label={t("Public account strategy factory module (strategy)")}><Input /></Form.Item>
      <Form.Item name="accountCompatibility" label={t("Optional account factory compatibility declaration (not certification)")} dependencies={['accountFactory']} rules={[({getFieldValue})=>({validator:async(_,value)=>{if(value && !getFieldValue('accountFactory')?.trim())throw new Error(t("Enter an account strategy factory first, or clear the declaration."));}})]}>
        <Select allowClear placeholder={t("Not declared")} options={[{value:'TRUSTED_STRATEGY_ONLY_SINGLE_NODE_V1',label:t("I explicitly declare: trusted strategy using only this node for its entire lifecycle")}]}/>
      </Form.Item>

    </Form>
    {pending && <p>{t("Save outcome needs reconciliation.")}</p>}
    <Button onClick={()=>void save()} loading={busy}>{pending ? t("Check original save request") : t("Save environment configuration only")}</Button>{' '}
    <Button onClick={()=>void inspect()} disabled={busy || !!pending || !saved?.environment}>{t("Explicitly inspect saved environment")}</Button>
    <p>{t("Building executes project code. Only Nautilus 0.69 (official git 634) is verified.")}</p>
    <details><summary>{t("Import a compatible launcher")}</summary>
      <p>{t("Import runs capabilities on the selected program; it does not start an account.")}</p>
      <Input aria-label={t("Import environment ID")} placeholder={t("Enter an environment ID if no source environment exists")} value={saved?.environment?.id??importId} disabled={!!saved?.environment||!!buildRequest||busy} onChange={e=>setImportId(e.target.value)}/>
      <Input aria-label={t("Absolute path to existing launcher")} placeholder={t("Absolute path accessible from the application container")} value={importPath} disabled={!!buildRequest||busy} onChange={e=>setImportPath(e.target.value)}/>
      <Button disabled={busy||!!pending||(!buildRequest&&(!importPath||!(saved?.environment?.id??importId)))} onClick={()=>confirm(t("Query capabilities of this trusted launcher?"),t("The selected program will run capabilities without sandboxing its code. No account connection or trading is initiated by this action."),()=>{void importEnvironment();})}>{buildRequest?.sourceExecutable?t("Check original import request"):t("Explicitly import and query capabilities")}</Button>
    </details>
    <Button onClick={()=>void buildEnvironment()} disabled={busy||!!pending||(!saved?.environment&&!buildRequest)}>{buildRequest?t("Check original preparation request"):t("Explicitly build launcher")}</Button>{' '}
    <Button onClick={()=>void readBuild()} disabled={busy||!buildRequest}>{t("Read original preparation progress only")}</Button>{' '}
    <Button onClick={()=>void selectRunner()} disabled={busy||build?.state.outcome?.status!=='completed'}>{t("Select verified artifact (no account start)")}</Button>
    {(build?.state.status==='terminal'||(error instanceof ApiError&&error.status<500))&&<Button disabled={busy} onClick={()=>{setBuildRequest(undefined);setBuild(undefined);localStorage.removeItem('qz-native-build-request');}}>{t("Prepare a new explicit build or import request")}</Button>}
    {buildRequest&&<p>{t("Original request:")}{buildRequest.requestId}</p>}
    {build&&<PreparationStatus value={build}/>}
    {saved?.runner&&<><AccountCompatibilityNotice declaration={saved.runner.capabilities.account.compatibilityDeclaration}/><SelectedRunnerStatus runner={saved.runner}/></>}

    <p><a href={href('/settings?section=accounts')}>{t("Go to account settings to add or view configurations")}</a></p>
    {notice && <p>{notice}</p>}{error !== undefined && <ErrorNotice error={error} />}
    {discovery !== undefined && <pre>{JSON.stringify(discovery,null,2)}</pre>}
  </div>;
}
