/* Design-only component prototype. All records are explicitly labelled examples.
   Uses official Ant Design components; never connects to a service or submits work. */
import { useState, type ReactNode } from 'react';
import { createRoot } from 'react-dom/client';
import { Alert, App, Button, ConfigProvider, Drawer, Form, Input, InputNumber, Modal, Segmented, Select, Table, Tabs, Tag, theme as antTheme } from 'antd';
import { ApiOutlined, AppstoreOutlined, AreaChartOutlined, ArrowLeftOutlined, DatabaseOutlined, DeploymentUnitOutlined, ExperimentOutlined, LinkOutlined, LogoutOutlined, MenuOutlined, MinusOutlined, PlusOutlined, ReloadOutlined, SearchOutlined, SettingOutlined } from '@ant-design/icons';
import './prototype.css';

type View = 'research' | 'new' | 'data' | 'portfolios' | 'observations' | 'settings' | 'extensions';
const params = new URLSearchParams(location.search);
const records = [
  { id: 18, title: '成交量变化能否解释短期收益？', status: '已计算', trials: 3 },
  { id: 17, title: '价格与成交量的短期联动', status: '研究中', trials: 2 },
  { id: 16, title: '极端波动后的均值回归', status: '独立评估失败', trials: 4 },
];
const nav: { view: View; label: string; icon: ReactNode; secondary?: boolean }[] = [
  { view: 'research', label: '研究', icon: <ExperimentOutlined aria-hidden /> },
  { view: 'data', label: '数据', icon: <DatabaseOutlined aria-hidden /> },
  { view: 'portfolios', label: '组合回测', icon: <AreaChartOutlined aria-hidden /> },
  { view: 'observations', label: '目标与观测', icon: <DeploymentUnitOutlined aria-hidden /> },
  { view: 'settings', label: '模型设置', icon: <ApiOutlined aria-hidden />, secondary: true },
  { view: 'extensions', label: '扩展', icon: <AppstoreOutlined aria-hidden />, secondary: true },
];
function State({ children, warning = false, error = false }: { children: ReactNode; warning?: boolean; error?: boolean }) {
  return <span className={`state ${warning ? 'warning' : error ? 'error' : ''}`}>{children}</span>;
}
function Facts({ items }: { items: [string, ReactNode][] }) {
  return <dl className="facts">{items.map(([label, value]) => <div key={label}><dt>{label}</dt><dd>{value}</dd></div>)}</dl>;
}
function Empty({ title, action }: { title: string; action?: ReactNode }) {
  return <div className="empty"><div className="empty-mark"><AreaChartOutlined aria-hidden /></div><h3>{title}</h3>{action}</div>;
}
function Prototype() {
  const [dark, setDark] = useState(params.get('theme') === 'dark');
  const [view, setView] = useState<View>((params.get('view') as View) || 'research');
  const [tab, setTab] = useState(params.get('tab') || 'evidence');
  const [selected, setSelected] = useState<number | null>(18);
  const [query, setQuery] = useState('');
  const [filter, setFilter] = useState('全部状态');
  const [menu, setMenu] = useState(false);
  const [dialog, setDialog] = useState<string | null>(null);
  const [expanded, setExpanded] = useState<number | null>(null);
  const [form] = Form.useForm();
  const colors = dark ? {
    canvas: '#111B16', surface: '#17251D', text: '#E9F0EB', secondary: '#A3B8A9', accent: '#8BC6A0', border: '#6B8473', fill: '#1E3026', onAccent: '#12251B',
  } : {
    canvas: '#F4F7F5', surface: '#FCFDFC', text: '#172A23', secondary: '#52645B', accent: '#246B4E', border: '#81948A', fill: '#EDF2EE', onAccent: '#FCFDFC',
  };
  const go = (next: View) => { setView(next); setMenu(false); };
  const navigation = <>
    <a className="wordmark" href="#research" onClick={e => { e.preventDefault(); go('research'); }}>QuaZonai<span>研究工作台</span></a>
    <nav aria-label="工作区">{nav.map(item => <a key={item.view} className={`nav-link ${item.secondary ? 'secondary' : ''} ${(view === item.view || view === 'new' && item.view === 'research') ? 'selected' : ''}`} aria-current={view === item.view ? 'page' : undefined} href={`#${item.view}`} onClick={e => { e.preventDefault(); go(item.view); }}>{item.icon}<span>{item.label}</span></a>)}</nav>
    <div className="rail-footer"><Button type="text" block icon={<SettingOutlined aria-hidden />} onClick={() => setDark(!dark)}>{dark ? '深色外观' : '浅色外观'}</Button><Button type="text" block icon={<LogoutOutlined aria-hidden />} onClick={() => setDialog('这是设计预览，不连接真实会话。')}>退出登录</Button></div>
  </>;
  const title = view === 'new' ? '新建研究' : nav.find(item => item.view === view)?.label || '研究';
  const study = records.find(item => item.id === selected) ?? records[0]!;
  const trialRows = [
    { id: 103, hypothesis: '按原政策检验成交量偏离信号', state: '已计算', metric: '不可用', reason: '分数恒定', selected: true },
    { id: 102, hypothesis: '比较短期价格与成交量变化', state: '失败', metric: '未提供', reason: 'native_forecast_failed', selected: false },
    { id: 101, hypothesis: '检验价格变化的条件关系', state: '已计算', metric: '不可用', reason: '分数恒定', selected: false },
  ];
  const trials = <section className="section-block"><div className="section-heading"><h3>试验记录</h3><span className="quiet">3 次尝试</span></div><Table size="small" rowKey="id" pagination={false} dataSource={trialRows} scroll={{ x: 620 }} expandable={{ columnWidth: 56, expandedRowKeys: expanded === null ? [] : [expanded], onExpand: (open, row) => setExpanded(open ? row.id : null), expandIcon: ({ expanded: open, onExpand, record }) => <Button type="text" className="trial-expander" aria-label={`${open ? '收起' : '展开'}试验 ${record.id}`} aria-expanded={open} icon={open ? <MinusOutlined /> : <PlusOutlined />} onClick={event => onExpand(record, event)} />, expandedRowRender: row => <div className="expanded-trial"><h4>{row.hypothesis}</h4><Facts items={[["试验身份", <code>{row.id}</code>], ["数据分区", '验证'], ["Pearson IC", '不可用'], ["原因", row.reason]]} /></div> }} columns={[
    { title: '假设', dataIndex: 'hypothesis', width: 245, render: (value, row) => <div className="trial-name">{value}<span className="quiet">试验 {row.id}</span></div> },
    { title: '状态', dataIndex: 'state', width: 85, render: value => <State error={value === '失败'}>{value}</State> },
    { title: '验证 IC', dataIndex: 'metric', width: 105, align: 'right', render: (value, row) => <div>{value}<small className="quiet block">{row.reason === '分数恒定' ? row.reason : ''}</small></div> },
    { title: '独立选择', dataIndex: 'selected', width: 95, render: value => value ? <span className="selection-text">所选试验</span> : <span className="quiet">未选择</span> },
  ]} /></section>;
  const evidence = <>
    <section className="evidence-panel">
      <div className="section-heading"><h3>独立评估</h3><State>已计算</State></div>
      <div className="selection-line"><span className="quiet">所选试验</span><button className="text-link" onClick={() => { setTab('trials'); setExpanded(103); }}>试验 103 <LinkOutlined aria-hidden /></button></div>
      <div className="metrics-grid">
        <div className="metric-primary"><span className="metric-label">独立样本 IC</span><strong>不可用</strong><span className="availability">分数恒定</span></div>
        <div className="metric-small"><span className="metric-label">有效观测</span><strong>2</strong></div>
        <div className="metric-small"><span className="metric-label">排除观测</span><strong>0</strong></div>
      </div>
      <div className="evidence-footer"><span>独立样本</span><span>分量 0</span></div>
    </section>
    <section className="section-block"><div className="section-heading"><h3>所选假设</h3></div><p className="hypothesis">按原政策检验成交量偏离信号</p><button className="text-link" onClick={() => { setTab('trials'); setExpanded(103); }}>查看完整试验 <LinkOutlined aria-hidden /></button></section>
    <div className="optional-action"><div><h3>组合回测</h3></div><Button icon={<PlusOutlined aria-hidden />} onClick={() => go('portfolios')}>创建组合</Button></div>
  </>;
  const research = <div className={`research-grid ${selected !== null ? 'has-selection' : ''}`}>
    <section className="collection" aria-label="研究列表"><div className="collection-tools"><label htmlFor="research-search">搜索已加载研究</label><Input id="research-search" prefix={<SearchOutlined aria-hidden />} allowClear value={query} onChange={e => setQuery(e.target.value)} placeholder="研究问题或模型" /><Select aria-label="研究状态" value={filter} onChange={setFilter} options={['全部状态', '研究中', '已计算', '独立评估失败'].map(value => ({ value }))} /><div className="collection-scope"><span>已加载 3 条</span>{(query || filter !== '全部状态') && <button className="text-link" onClick={() => { setQuery(''); setFilter('全部状态'); }}>清除筛选</button>}</div></div><div className="study-list">{records.filter(row => row.title.includes(query) && (filter === '全部状态' || filter === row.status)).map(row => <a className={`study-row ${selected === row.id ? 'active' : ''}`} key={row.id} href={`#study-${row.id}`} onClick={e => { e.preventDefault(); setSelected(row.id); }} aria-current={selected === row.id ? 'true' : undefined}><h2>{row.title}</h2><State error={row.status.includes('失败')}>{row.status}</State><div className="study-meta"><span>试验 {row.trials} 次</span><span>Faux Provider</span></div></a>)}</div></section>
    {selected === null ? <div className="detail-panel"><Empty title="选择一项研究" /></div> : <article className="detail-panel"><button className="mobile-back text-link" onClick={() => setSelected(null)}><ArrowLeftOutlined /> 研究列表</button><header className="study-heading"><div className="record-kicker">研究 {study.id}</div><h2>{study.title}</h2><div className="study-heading-meta"><State error={study.status.includes('失败')}>{study.status}</State><span>试验 {study.trials} 次</span><span>Faux Provider</span></div></header><Tabs activeKey={tab} onChange={setTab} items={[
      { key: 'evidence', label: '证据', children: study.id === 17 ? <Empty title="尚无独立结果" action={<Button onClick={() => setTab('trials')}>查看试验</Button>} /> : study.id === 16 ? <div className="section-block"><Alert type="error" showIcon title="独立评估失败" description={<button className="text-link" onClick={() => setDialog('native_forecast_failed')}>技术详情</button>} />{trials}</div> : evidence },
      { key: 'trials', label: '试验', children: trials },
      { key: 'inputs', label: '输入与执行', children: <div className="section-block"><h3>研究输入</h3><Facts items={[["数据集", '协议测试数据'], ["来源", '受控协议测试'], ["模型", 'Faux Provider'], ["推理强度", '关闭'], ["试验预算", '4 次'], ["覆盖区间", '未提供']]} /><div className="group-divider" /><h3>最近执行记录</h3><Facts items={[["原始提交", <code>submission 18</code>], ["提交状态", '已回答'], ["原生任务", 'completed'], ["更新时间", '未记录']]} /></div> },
    ]} /></article>}
  </div>;
  const newStudy = <div className="form-surface"><Form form={form} layout="vertical" onFinish={() => setDialog('原型不提交研究。')} initialValues={{ dataset: 'controlled', model: 'faux', thinking: 'off' }}><Form.Item label="研究问题" name="goal" rules={[{ required: true, message: '请输入研究问题' }]}><Input.TextArea rows={4} placeholder="例如：成交量变化能否解释短期收益？" /></Form.Item><Form.Item label="数据集" name="dataset"><Select options={[{ value: 'controlled', label: '协议测试数据' }]} /></Form.Item><div className="selected-context"><span>受控协议测试</span><span>USD</span><State>已就绪</State></div><div className="form-columns"><Form.Item label="模型" name="model"><Select options={[{ value: 'faux', label: 'Faux Provider' }]} /></Form.Item><Form.Item label="推理强度" name="thinking"><Select options={[{ value: 'off', label: '关闭' }]} /></Form.Item></div><Form.Item label="试验预算" name="budget" rules={[{ required: true, message: '请输入试验预算' }]} extra="失败试验也计入预算"><InputNumber min={0} precision={0} suffix="次" placeholder="输入次数" style={{ width: '100%' }} /></Form.Item><div className="form-actions"><Button onClick={() => go('research')}>取消</Button><Button type="primary" htmlType="submit">开始研究</Button></div></Form></div>;
  const data = <div className="surface"><Table rowKey="id" pagination={false} scroll={{ x: 800 }} dataSource={[{ id: 'controlled', name: '协议测试数据', source: '受控协议测试', asset: 'TEST', coverage: '未提供', currency: 'USD' }]} columns={[
    { title: '数据集', dataIndex: 'name', render: (name, row) => <div className="trial-name">{name}<code className="quiet">{row.id}</code></div> },
    { title: '来源', dataIndex: 'source' }, { title: '资产', dataIndex: 'asset' }, { title: '覆盖区间', dataIndex: 'coverage' }, { title: '币种', dataIndex: 'currency' },
    { title: '状态', render: () => <State>已就绪</State> }, { title: '操作', render: () => <Button onClick={() => go('new')}>用于研究</Button> },
  ]} /></div>;
  const portfolios = <div className="portfolio-layout"><div className="surface portfolio-form"><Form layout="vertical" onFinish={() => setDialog('原型不运行回测。')} initialValues={{ name: '成交量信号组合', data: 'controlled', capital: '1000', weight: '1' }}><h2>创建组合</h2><Form.Item label="组合名称" name="name"><Input /></Form.Item><Form.Item label="数据集" name="data"><Select options={[{ value: 'controlled', label: '协议测试数据' }]} /></Form.Item><div className="member-block"><span>所选研究</span><strong>成交量变化能否解释短期收益？</strong><Form.Item label="权重" name="weight"><Input inputMode="decimal" /></Form.Item></div><Form.Item label="本金" name="capital"><Input suffix="USD" inputMode="decimal" /></Form.Item><div className="form-actions"><Button type="primary" htmlType="submit">运行回测</Button></div></Form></div></div>;
  const observations = <div className="observation-page"><div className="record-topline"><div><div className="record-kicker">协议示例连接</div><h2>成交量信号组合</h2><div className="study-heading-meta"><Tag>模拟</Tag><span>协议测试账户</span></div></div><Button type="primary" icon={<ReloadOutlined aria-hidden />} onClick={() => setDialog('原型不核对真实回执。')}>核对回执</Button></div><section className="surface receipt-surface"><div className="section-heading"><h3>目标回执</h3><State warning>待核对</State></div><Facts items={[["回执身份", <code>protocol-receipt</code>], ["到期时间", '未提供'], ["下游身份", '未提供'], ["接收时间", '未提供']]} /><button className="text-link" onClick={() => setDialog('原型示例，没有真实下游目标。')}>来源与身份</button></section><section className="surface snapshot-surface"><div className="section-heading"><h3>账户快照</h3><span className="quiet">尚无快照</span></div><div className="snapshot-values"><div><span>权益 / USD</span><strong>未提供</strong></div><div><span>现金 / USD</span><strong>未提供</strong></div></div><div className="chart-empty"><span>尚无账户快照</span></div></section><div className="observation-footer"><h3>持仓</h3><span className="quiet">尚无快照</span><Button danger onClick={() => setDialog('仅停止本地观测，不撤回下游目标。')}>停止观测</Button></div></div>;
  const settings = <div className="settings-layout"><section className="surface settings-section"><div className="section-heading"><h2>模型连接</h2></div><div className="provider-row"><div className="provider-icon"><ApiOutlined aria-hidden /></div><div><strong>Faux Provider</strong><span className="quiet block">原型示例</span></div><State>已配置</State><Button onClick={() => setDialog('原型不更改模型凭据。')}>断开</Button></div></section><section className="surface settings-section"><h2>研究默认值</h2><Form layout="vertical" initialValues={{ model: 'faux', reasoning: 'off' }} onFinish={() => setDialog('原型不保存真实设置。')}><div className="form-columns"><Form.Item label="默认模型" name="model"><Select options={[{ value: 'faux', label: 'Faux Provider' }]} /></Form.Item><Form.Item label="推理强度" name="reasoning"><Select options={[{ value: 'off', label: '关闭' }]} /></Form.Item></div><Button type="primary" htmlType="submit">保存设置</Button></Form></section><section className="surface settings-section"><h2>外观</h2><Segmented options={['浅色', '深色']} value={dark ? '深色' : '浅色'} onChange={value => setDark(value === '深色')} /></section><section className="settings-section usage-section"><h2>工作台用量</h2><Facts items={[["输入 Token", '未提供'], ["输出 Token", '未提供'], ["费用", '未提供'], ["活动任务", '0']]} /></section></div>;
  const extensions = <div className="surface"><Table rowKey="name" pagination={false} scroll={{ x: 660 }} dataSource={[
    { name: 'qz.science', kind: 'science', version: '1', enabled: '必需', state: '已加载' },
    { name: 'test.protocol-research', kind: 'research', version: '未提供', enabled: '已停用', state: '待重启' },
  ]} columns={[
    { title: '扩展名称', dataIndex: 'name', render: value => <code>{value}</code> }, { title: '类型', dataIndex: 'kind' }, { title: '版本', dataIndex: 'version' },
    { title: '启用设置', dataIndex: 'enabled' }, { title: '当前状态', dataIndex: 'state', render: value => <State warning={value === '待重启'}>{value}</State> },
  ]} /></div>;
  return <ConfigProvider button={{ autoInsertSpace: false }} theme={{ algorithm: dark ? antTheme.darkAlgorithm : antTheme.defaultAlgorithm, token: { colorPrimary: colors.accent, colorTextLightSolid: colors.onAccent, colorBgBase: colors.surface, colorBgContainer: colors.surface, colorBgLayout: colors.canvas, colorText: colors.text, colorTextSecondary: colors.secondary, colorTextPlaceholder: colors.secondary, colorBorder: colors.border, colorFillAlter: colors.fill, colorLink: colors.accent, colorError: dark ? '#F19B93' : '#B0403E', colorErrorText: dark ? '#F19B93' : '#B0403E', colorErrorBorder: dark ? '#F19B93' : '#B0403E', borderRadius: 8, controlHeight: 40, fontSize: 14, fontFamily: '"Geist", "Noto Sans CJK SC", "Microsoft YaHei", system-ui, sans-serif', motion: false }, components: { Button: { primaryShadow: 'none', defaultShadow: 'none', dangerShadow: 'none' }, Table: { headerBg: colors.fill, cellPaddingBlock: 14 }, Tabs: { horizontalItemGutter: 28 } } }}><App><div className="prototype" data-theme={dark ? 'dark' : 'light'}><div className="prototype-label">设计原型 / 示例数据 / 不连接服务</div><a href="#main-content" className="skip-link">跳到内容</a><aside className="rail">{navigation}</aside><header className="mobile-header"><Button type="text" icon={<MenuOutlined />} aria-label="打开导航" onClick={() => setMenu(true)} /><span>QuaZonai</span><Button type="text" aria-label="切换主题" icon={<SettingOutlined />} onClick={() => setDark(!dark)} /></header><Drawer title="QuaZonai" placement="left" getContainer={false} open={menu} onClose={() => setMenu(false)} width={280}>{navigation}</Drawer><main id="main-content"><div className="page-heading">{view === 'new' && <button className="text-link form-back" onClick={() => go('research')}><ArrowLeftOutlined /> 研究</button>}<div className="page-heading-row"><h1>{title}</h1>{view === 'research' && <Button type="primary" icon={<PlusOutlined aria-hidden />} onClick={() => go('new')}>新建研究</Button>}{view === 'data' && <span className="quiet">1 个已登记数据集</span>}{view === 'observations' && <Button icon={<PlusOutlined aria-hidden />} onClick={() => setDialog('原型仅预览已有示例观测。')}>新建观测</Button>}</div></div>{view === 'research' ? research : view === 'new' ? newStudy : view === 'data' ? data : view === 'portfolios' ? portfolios : view === 'observations' ? observations : view === 'settings' ? settings : extensions}</main><Modal getContainer={false} open={dialog !== null} title="设计预览" onCancel={() => setDialog(null)} footer={<Button onClick={() => setDialog(null)}>关闭</Button>}><p>{dialog}</p></Modal></div></App></ConfigProvider>;
}
createRoot(document.getElementById('root')!).render(<Prototype />);
