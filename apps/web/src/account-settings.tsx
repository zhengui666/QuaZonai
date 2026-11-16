import type { InstanceSettingsView } from '../../../packages/contracts/src/index.ts';
import AccountConfiguration from './account-configuration';
import { useResource } from './data-hooks';
import { href } from './router';
import { ErrorNotice, Loading } from './ui';

/** Reuses the existing account configuration API; opening settings never boots an account. */
export default function AccountSettings() {
  const instance = useResource<InstanceSettingsView>('/settings/instances');
  return <div className="account-settings">
    <h2>账户</h2>
    <p>分别保存和查看模拟盘、实盘及远程账户配置。保存配置不等于连接账户或启动策略。</p>
    <p>账户标识与钱包地址不同。同一实际账户不能重复创建执行宿主；实盘签名仍使用运行环境已有配置，此处不接收私钥，也不会创建新钱包。</p>
    <p><a href={href('/settings?section=instances')}>运行器与实例连接设置</a></p>
    {instance.isPending && <Loading />}
    <ErrorNotice error={instance.error}/>
    <AccountConfiguration enabled={instance.isSuccess && instance.data.runner?.capabilities.account.compiled === true}/>
  </div>;
}
