import {PreparationStatus,SelectedRunnerStatus,type PreparationView} from './native-preparation-status';
import { href } from './router';
import { Button, Checkbox, Form, Input, Select } from 'antd';
import { useEffect, useState } from 'react';
import type { InstanceSettingsView, NativeEnvironmentSettingsUpdate } from '../../../packages/contracts/src/index.ts';
import type { NativeRustEnvironment } from '../../../packages/contracts/src/nautilus.ts';
import { ApiError, api } from './api';
import { useResource } from './data-hooks';
import { ErrorNotice, Loading,useConfirm } from './ui';

type Fields = Omit<NativeRustEnvironment, 'kind' | 'features'> & { featureNames?: string };
export function environmentFromFields(values: Fields): NativeRustEnvironment {
  const accountFactory=values.accountFactory?.trim();
  if(values.accountCompatibility!==undefined && (values.accountCompatibility!=='TRUSTED_STRATEGY_ONLY_SINGLE_NODE_V1'||!accountFactory))throw new Error('兼容声明需要账户策略工厂；不声明时请清除此选项。');
  if(values.backtestCompletion!==undefined && (typeof values.backtestCompletion!=='boolean'||values.backtestCompletion&&!values.backtestFactory?.trim()))throw new Error('完成回调需要回测工厂和明确布尔值。');
  return {kind:'RUST_PROCESS',id:values.id.trim(),cargoBinary:values.cargoBinary.trim(),manifestPath:values.manifestPath.trim(),packageName:values.packageName.trim(),
    features:(values.featureNames??'').split(',').map(v=>v.trim()).filter(Boolean),
    ...(values.backtestFactory?.trim()?{backtestFactory:values.backtestFactory.trim()}:{}),...(accountFactory?{accountFactory}:{}),
    ...(values.backtestCompletion!==undefined?{backtestCompletion:values.backtestCompletion}:{}),
    ...(values.accountCompatibility?{accountCompatibility:values.accountCompatibility}:{}),
  };
}
export function AccountCompatibilityNotice({declaration}:{declaration:unknown}) {
  return <p>{declaration==='TRUSTED_STRATEGY_ONLY_SINGLE_NODE_V1'?'可信策略单节点：自报声明，未验证':'可信策略单节点：未声明'}</p>;
}
export function retryUncertainEnvironmentSave(status?: number) { return status === undefined || status >= 500; }
type BuildRequest={requestId:string;expectedRevision:number;environmentId?:string;sourceExecutable?:string};
type BuildView=PreparationView;
const required = [{ required: true, whitespace: true, message: '请填写此项' }];
export default function InstanceSettings() {
  const query = useResource<InstanceSettingsView>('/settings/instances');
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
      setSaved(result); setPending(undefined); setDiscovery(undefined); setNotice('配置已保存，请重新选择产物。未连接账户。');
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
    if(!request.sourceExecutable){setError(new Error('请先核实或结束原构建请求，再准备导入。'));return;}
    setBusy(true);setError(undefined);setBuildRequest(request);localStorage.setItem('qz-native-build-request',JSON.stringify(request));
    try{setBuild(await api<BuildView>('/settings/instances/import',{method:'POST',body:request}));}catch(failure){setError(failure);}finally{setBusy(false);}
  }
  async function readBuild(){if(!buildRequest||busy)return;setBusy(true);setError(undefined);try{setBuild(await api<BuildView>(`/settings/instances/build/${encodeURIComponent(buildRequest.requestId)}`));}catch(failure){setError(failure);}finally{setBusy(false);}}
  async function selectRunner(){if(!buildRequest||!saved||busy)return;setBusy(true);setError(undefined);try{setSaved(await api<InstanceSettingsView>('/settings/instances/select-runner',{method:'POST',body:{requestId:buildRequest.requestId,expectedRevision:buildRequest.expectedRevision}}));setNotice('产物已选用，未连接账户。');}catch(failure){setError(failure);}finally{setBusy(false);}}
  if (!loaded) return query.error ? <ErrorNotice error={query.error} /> : <Loading />;
  return <div className="settings-page instance-settings"><header className="page-heading"><h2>Nautilus 原生 Rust 环境</h2></header>

    <p>路径须可被应用运行环境访问。</p>

    {!!saved && (!!saved.backtest || saved.connections.length > 0) && <p>旧 HTTP 配置已保留。</p>}
    <Form form={form} layout="vertical" disabled={busy || !!pending}>
      <Form.Item name="id" label="环境标识" rules={required}><Input /></Form.Item>
      <Form.Item name="cargoBinary" label="已有 Cargo 可执行文件绝对路径" rules={required}><Input /></Form.Item>
      <Form.Item name="manifestPath" label="已有 Cargo.toml 绝对路径" rules={required}><Input /></Form.Item>
      <Form.Item name="packageName" label="策略库 package 名称" rules={required}><Input /></Form.Item>
      <Form.Item name="featureNames" label="显式 features（逗号分隔）"><Input /></Form.Item>
      <Form.Item name="backtestFactory" label="回测工厂公开模块（config / install）"><Input /></Form.Item>
      <Form.Item name="backtestCompletion" valuePropName="checked" label="原生运行返回后的完成检查回调" extra="显式启用同模块 finalize_after_node_run；需重新构建，不把停止当自然 EOF。"><Checkbox>启用</Checkbox></Form.Item>
      <Form.Item name="accountFactory" label="账户策略工厂公开模块（strategy）"><Input /></Form.Item>
      <Form.Item name="accountCompatibility" label="可选：账户工厂兼容声明（不是认证）" dependencies={['accountFactory']} rules={[({getFieldValue})=>({validator:async(_,value)=>{if(value && !getFieldValue('accountFactory')?.trim())throw new Error('请先填写账户策略工厂，或清除声明。');}})]}>
        <Select allowClear placeholder="未声明" options={[{value:'TRUSTED_STRATEGY_ONLY_SINGLE_NODE_V1',label:'我明确声明：可信策略，整个生命周期只使用此单节点'}]}/>
      </Form.Item>

    </Form>
    {pending && <p>保存结果待核对。</p>}
    <Button onClick={()=>void save()} loading={busy}>{pending ? '核实原保存请求' : '仅保存环境配置'}</Button>{' '}
    <Button onClick={()=>void inspect()} disabled={busy || !!pending || !saved?.environment}>显式检查已保存环境</Button>
    <p>构建将执行项目代码。仅验证 Nautilus 0.69（官方 git 634）。</p>
    <details><summary>导入兼容启动器</summary>
      <p>导入将执行所选程序的 capabilities，不启动账户。</p>
      <Input aria-label="导入环境标识" placeholder="没有源码环境时填写环境标识" value={saved?.environment?.id??importId} disabled={!!saved?.environment||!!buildRequest||busy} onChange={e=>setImportId(e.target.value)}/>
      <Input aria-label="已有启动器绝对路径" placeholder="应用容器可访问的绝对路径" value={importPath} disabled={!!buildRequest||busy} onChange={e=>setImportPath(e.target.value)}/>
      <Button disabled={busy||!!pending||(!buildRequest&&(!importPath||!(saved?.environment?.id??importId)))} onClick={()=>confirm('执行可信启动器的能力查询？','将运行所选程序的 capabilities，未隔离用户代码；不会主动连接账户或交易。',()=>{void importEnvironment();})}>{buildRequest?.sourceExecutable?'核实原导入请求':'显式导入并执行能力查询'}</Button>
    </details>
    <Button onClick={()=>void buildEnvironment()} disabled={busy||!!pending||(!saved?.environment&&!buildRequest)}>{buildRequest?'核实原准备请求':'显式构建启动器'}</Button>{' '}
    <Button onClick={()=>void readBuild()} disabled={busy||!buildRequest}>只读取原准备进度</Button>{' '}
    <Button onClick={()=>void selectRunner()} disabled={busy||build?.state.outcome?.status!=='completed'}>选用已核实产物（不启动账户）</Button>
    {(build?.state.status==='terminal'||(error instanceof ApiError&&error.status<500))&&<Button disabled={busy} onClick={()=>{setBuildRequest(undefined);setBuild(undefined);localStorage.removeItem('qz-native-build-request');}}>准备新的显式构建或导入请求</Button>}
    {buildRequest&&<p>原请求：{buildRequest.requestId}</p>}
    {build&&<PreparationStatus value={build}/>}
    {saved?.runner&&<><AccountCompatibilityNotice declaration={saved.runner.capabilities.account.compatibilityDeclaration}/><SelectedRunnerStatus runner={saved.runner}/></>}

    <p><a href={href('/settings?section=accounts')}>前往账户设置，添加或查看账户配置</a></p>
    {notice && <p>{notice}</p>}{error !== undefined && <ErrorNotice error={error} />}
    {discovery !== undefined && <pre>{JSON.stringify(discovery,null,2)}</pre>}
  </div>;
}
