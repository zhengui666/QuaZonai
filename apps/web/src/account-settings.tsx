import { useState } from 'react';
import { Select } from 'antd';
import type { InstanceSettingsView } from '../../../packages/contracts/src/index.ts';
import AccountConfiguration from './account-configuration';
import { useResource } from './data-hooks';
import { href } from './router';
import { ErrorNotice, Loading } from './ui';

type AccountProvider = 'HYPERLIQUID' | 'INTERACTIVE_BROKERS' | 'COINBASE';
export function ProviderAccountConfiguration({provider,enabled}:{provider:AccountProvider;enabled:boolean}) {
  if(provider==='HYPERLIQUID') return <>
    <p>Hyperliquid：当前已接入的账户运行器。账户标识与钱包地址不同；此处不接收私钥，也不会创建新钱包。</p>
    <AccountConfiguration enabled={enabled}/>
  </>;
  return <section aria-label="官方适配器接入状态">
    <h3>{provider==='INTERACTIVE_BROKERS'?'Interactive Brokers（IBKR）':'Coinbase Advanced Trade'}</h3>
    <p>Nautilus 官方适配器已存在，QuaZonai 尚未接入此账户类型。当前不能在这里保存、连接或启动该账户，不会改用 Hyperliquid 运行器。</p>
    {provider==='INTERACTIVE_BROKERS'
      ?<p>官方连接使用已启动的 TWS 或 IB Gateway，以及 host、port、API client ID、真实 IB 账户代码。券商模拟账户与本地 Sandbox 不同。</p>
      :<p>官方适配器区分现货 Cash 与 CFM 衍生品 Margin，使用 Coinbase API key/secret 和可选 portfolio 配置。不能把 Hyperliquid 钱包地址或凭据引用用于此账户。</p>}
  </section>;
}
/** Reuses the existing account configuration API; opening settings never boots an account. */
export default function AccountSettings() {
  const instance = useResource<InstanceSettingsView>('/settings/instances');
  const [provider,setProvider]=useState<AccountProvider>('HYPERLIQUID');
  return <div className="account-settings">
    <h2>账户</h2>
    <p>按券商或交易所管理真实交易账户。账户类型、运行实例与签名凭据是不同配置；保存不等于连接或启动策略。</p>
    <label>券商 / 交易所<Select aria-label="券商 / 交易所" value={provider} onChange={setProvider} options={[
      {value:'HYPERLIQUID',label:'Hyperliquid · 已接入'},
      {value:'INTERACTIVE_BROKERS',label:'Interactive Brokers（IBKR）· 尚未接入'},
      {value:'COINBASE',label:'Coinbase Advanced Trade · 尚未接入'},
    ]}/></label>
    <p><a href={href('/settings?section=instances')}>运行器与实例连接设置</a></p>
    {instance.isPending && <Loading />}
    <ErrorNotice error={instance.error}/>
    <ProviderAccountConfiguration provider={provider} enabled={instance.isSuccess && instance.data.runner?.capabilities.account.compiled === true}/>
  </div>;
}
