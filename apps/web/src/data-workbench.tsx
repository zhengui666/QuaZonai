import { App, Tabs } from 'antd';
import { useContext, useState } from 'react';
import { DataManagement } from './data';
import { DataInputs } from './data-inputs';
import { GuardContext } from './ui';

export function DataWorkbench() {
  const [tab, setTab] = useState('registry');
  const { blocked } = useContext(GuardContext); const { message } = App.useApp();
  return <>
    <Tabs activeKey={tab} onChange={next => {
      if (next === tab) return;
      if (blocked) { void message.info('请先保存或取消更改'); return; }
      setTab(next);
    }} items={[{ key: 'registry', label: '数据登记' }, { key: 'inputs', label: '冻结输入' }]} />
    {tab === 'registry' ? <DataManagement /> : <DataInputs />}
  </>;
}
