import AccountConfiguration from './account-configuration';
import { Button, Form, Input } from 'antd';
import { useEffect, useState } from 'react';
import type { InstanceSettingsView, NativeEnvironmentSettingsUpdate } from '../../../packages/contracts/src/index.ts';
import type { NativeRustEnvironment } from '../../../packages/contracts/src/nautilus.ts';
import { ApiError, api } from './api';
import { useResource } from './data-hooks';
import { ErrorNotice, Loading } from './ui';

type Fields = Omit<NativeRustEnvironment, 'kind' | 'features'> & { featureNames?: string };
export function retryUncertainEnvironmentSave(status?: number) { return status === undefined || status >= 500; }
type BuildView={requestId:string;taskId:number;state:{status:string;outcome?:{status:string;result?:{qualification:string;target:{capabilities:unknown}};error?:{message:string}}}};
const required = [{ required: true, whitespace: true, message: '请填写此项' }];
export default function InstanceSettings() {
  const query = useResource<InstanceSettingsView>('/settings/instances');
  const [form] = Form.useForm<Fields>();
  const [loaded, setLoaded] = useState(false), [busy, setBusy] = useState(false);
  const [saved, setSaved] = useState<InstanceSettingsView>();
  const [pending, setPending] = useState<NativeEnvironmentSettingsUpdate>();
  const [error, setError] = useState<unknown>(); const [notice, setNotice] = useState('');
  const [discovery, setDiscovery] = useState<unknown>();
  const [buildRequest,setBuildRequest]=useState<{requestId:string;expectedRevision:number}|undefined>(()=>{try{return JSON.parse(localStorage.getItem('qz-native-build-request')??'null')??undefined;}catch{return undefined;}});
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
      const request = pending ?? { expectedRevision:saved.revision, environment:{ kind:'RUST_PROCESS' as const, id:values!.id.trim(), cargoBinary:values!.cargoBinary.trim(), manifestPath:values!.manifestPath.trim(), packageName:values!.packageName.trim(),
        features:(values!.featureNames ?? '').split(',').map(v=>v.trim()).filter(Boolean),
        ...(values!.backtestFactory?.trim() ? {backtestFactory:values!.backtestFactory.trim()} : {}), ...(values!.accountFactory?.trim() ? {accountFactory:values!.accountFactory.trim()} : {}) } };
      setBusy(true); setPending(request);
      const result = await api<InstanceSettingsView>('/settings/instances',{method:'PUT',body:request});
      setSaved(result); setPending(undefined); setDiscovery(undefined); setNotice('环境配置已保存。没有构建、连接账户或启动策略。');
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
    if(busy||pending||!saved?.environment)return;setBusy(true);setError(undefined);
    const request=buildRequest??{requestId:crypto.randomUUID(),expectedRevision:saved.revision};
    setBuildRequest(request);localStorage.setItem('qz-native-build-request',JSON.stringify(request));
    try{setBuild(await api<BuildView>('/settings/instances/build',{method:'POST',body:request}));}
    catch(failure){setError(failure);}finally{setBusy(false);}
  }
  async function readBuild(){if(!buildRequest||busy)return;setBusy(true);setError(undefined);try{setBuild(await api<BuildView>(`/settings/instances/build/${encodeURIComponent(buildRequest.requestId)}`));}catch(failure){setError(failure);}finally{setBusy(false);}}
  async function selectRunner(){if(!buildRequest||!saved||busy)return;setBusy(true);setError(undefined);try{setSaved(await api<InstanceSettingsView>('/settings/instances/select-runner',{method:'POST',body:{requestId:buildRequest.requestId,expectedRevision:buildRequest.expectedRevision}}));setNotice('已选用成功构建的启动器。尚未连接账户或启动策略。');}catch(failure){setError(failure);}finally{setBusy(false);}}
  if (!loaded) return query.error ? <ErrorNotice error={query.error} /> : <Loading />;
  return <div className="settings-page instance-settings"><header className="page-heading"><h2>Nautilus 原生 Rust 环境</h2></header>
    <p>只有一种连接方式：使用你已有的 Rust 项目构建薄启动器，通过进程连接查看与控制 Nautilus。QuaZonai 不随应用捆绑 Nautilus，不修改其源码，不要求预装常驻适配服务。</p>
    <p>路径必须能被 QuaZonai 服务所在机器访问；默认容器没有 Cargo / rustc 或宿主项目挂载，不能直接填写宿主路径当作可用环境。容器需要所有者准备兼容的已有工具链、系统库和挂载，宿主与容器 glibc 不一致也可能无法运行。这里不会通过 SSH 自动访问远程机器，也不会接管已运行且未提供接口的节点。</p>
    <p>保存配置、检查或构建环境、连接账户、启动交易是不同操作。单一环境可承载独立研究进程和显式账户进程，不代表只有一个操作系统进程；研究权限不是交易许可。</p>
    {!!saved && (!!saved.backtest || saved.connections.length > 0) && <p>检测到旧 HTTP 配置，已作为迁移事实保留，不再提供新的 HTTP 配置入口。旧任务、账户及未决回执不会改写成新连接，UNKNOWN 不会自动重发或清除。</p>}
    <Form form={form} layout="vertical" disabled={busy || !!pending}>
      <Form.Item name="id" label="环境标识" rules={required}><Input /></Form.Item>
      <Form.Item name="cargoBinary" label="已有 Cargo 可执行文件绝对路径" rules={required}><Input /></Form.Item>
      <Form.Item name="manifestPath" label="已有 Cargo.toml 绝对路径" rules={required}><Input /></Form.Item>
      <Form.Item name="packageName" label="策略库 package 名称" rules={required}><Input /></Form.Item>
      <Form.Item name="featureNames" label="显式 features（逗号分隔）"><Input /></Form.Item>
      <Form.Item name="backtestFactory" label="回测工厂公开模块（config / install）"><Input /></Form.Item>
      <Form.Item name="accountFactory" label="账户策略工厂公开模块（strategy）"><Input /></Form.Item>
    </Form>
    {pending && <p>保存结果尚未确认。重试保留原配置与版本，不创建另一请求。</p>}
    <Button onClick={()=>void save()} loading={busy}>{pending ? '核实原保存请求' : '仅保存环境配置'}</Button>{' '}
    <Button onClick={()=>void inspect()} disabled={busy || !!pending || !saved?.environment}>显式检查已保存环境</Button>
    <p>检查仅读取 Cargo 清单。显式构建会在独立目录执行你项目的 Rust / build.rs 代码，使用已有离线依赖，不下载或替换 Nautilus；路径、版本或精度不兼容会失败。构建成功仍不代表已连接账户或可交易。</p>
    <Button onClick={()=>void buildEnvironment()} disabled={busy||!!pending||!saved?.environment}>{buildRequest?'核实原构建请求':'显式构建启动器'}</Button>{' '}
    <Button onClick={()=>void readBuild()} disabled={busy||!buildRequest}>只读取原构建进度</Button>{' '}
    <Button onClick={()=>void selectRunner()} disabled={busy||build?.state.outcome?.status!=='completed'}>选用成功构建产物（不启动账户）</Button>
    {build?.state.status==='terminal'&&<Button disabled={busy} onClick={()=>{setBuildRequest(undefined);setBuild(undefined);localStorage.removeItem('qz-native-build-request');}}>准备新的显式构建请求</Button>}
    {buildRequest&&<p>原构建请求：{buildRequest.requestId}。重开或查询不会重编；结果不确定时保留原身份，不将它改称成功。</p>}
    {build&&<pre>{JSON.stringify(build,null,2)}</pre>}
    {saved?.runner&&<><p>已选用编译产物。此处 running:false 是能力查询进程的值，不表示任何账户已离线或对账完成。</p><pre>{JSON.stringify(saved.runner.capabilities,null,2)}</pre></>}
    <p>当前 QZ 账户接线仅支持 Hyperliquid、单账户中的一个 EMA 或已注册 Rust Strategy，最多 256 个标的，不等于任意官方 LiveNode 配置。账户工厂未编入时不能恢复账户；研究工厂可用不构成交易权限。连接与启动交易仍须账户页各自显式操作。</p>
    <AccountConfiguration enabled={saved?.runner?.capabilities.account.compiled===true}/>
    {notice && <p>{notice}</p>}{error !== undefined && <ErrorNotice error={error} />}
    {discovery !== undefined && <pre>{JSON.stringify(discovery,null,2)}</pre>}
  </div>;
}
