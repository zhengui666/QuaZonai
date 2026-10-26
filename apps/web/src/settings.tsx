import type { ModelLoginRequest } from './model-login';
import { href, type Route } from './router';
import ModelSettings from './model-settings';
import Extensions from './extensions';
import InstanceSettings from './instance-settings';
import Data from './data';
import DataPlugins from './data-plugins';

const sections = [{ id: 'models', label: '模型设置' }, { id: 'data', label: '数据' }, { id: 'data-plugins', label: '数据插件' }, { id: 'extensions', label: '扩展' }, { id: 'instances', label: '实例连接' }];
export default function Settings({ connect, route }: { connect: (value: ModelLoginRequest) => void; route: Route }) {
  const requested = route.page === 'data' ? 'data' : route.page === 'extensions' ? 'extensions' : route.params.get('section');
  const section = sections.some(item => item.id === requested) ? requested : 'models';
  return <div className="settings-page settings-hub">
    <header className="page-heading"><h1>设置</h1></header>
    <nav className="settings-nav" aria-label="设置分类">{sections.map(item => <a key={item.id} href={href(`/settings?section=${item.id}`)} aria-current={section === item.id ? 'page' : undefined}>{item.label}</a>)}</nav>
    {section === 'models' ? <ModelSettings connect={connect} /> : section === 'data' ? <Data /> : section === 'data-plugins' ? <DataPlugins /> : section === 'extensions' ? <Extensions /> : <InstanceSettings />}
  </div>;
}
