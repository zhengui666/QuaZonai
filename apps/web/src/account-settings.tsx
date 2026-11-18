import { nativeAccountAdapters } from '../../../packages/contracts/src/account-adapters.ts';
import { useTranslation } from 'react-i18next';
import type { InstanceSettingsView } from '../../../packages/contracts/src/index.ts';
import AccountConfiguration from './account-configuration';
import { useResource } from './data-hooks';
import { href } from './router';
import { ErrorNotice, Loading } from './ui';

/** Reuses the existing account configuration API; opening settings never boots an account. */
export default function AccountSettings() {
 const { t } = useTranslation('settings');

  const instance = useResource<InstanceSettingsView>('/settings/instances');
  return <div className="account-settings">
    <h2>{t("Accounts")}</h2>
    <p>{t("Save and inspect paper, live, and remote account configurations separately. Saving does not connect an account or start a strategy.")}</p>
    <p>{t("An account ID is not a wallet address. Do not create duplicate execution hosts for the same real account. Live signing uses existing runtime configuration; this page accepts no private keys and creates no wallets.")}</p>
    <p><a href={href('/settings?section=instances')}>{t("Runner and instance connections")}</a></p>
    {instance.isPending && <Loading />}
    <ErrorNotice error={instance.error}/>
    <AccountConfiguration adapters={instance.data?.runner ? nativeAccountAdapters(instance.data.runner.capabilities.account) : []} enabled={instance.isSuccess && instance.data.runner?.capabilities.account.compiled === true}/>
  </div>;
}
