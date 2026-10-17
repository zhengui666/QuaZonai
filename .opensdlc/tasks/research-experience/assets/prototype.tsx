/* Design-only prototype. Example records never connect to a service. */
import { useEffect, useState, type ReactNode } from 'react';
import { createRoot } from 'react-dom/client';
import {
  App, Button, ConfigProvider, Drawer, Dropdown, Form, Input, InputNumber,
  Modal, Segmented, Select, Switch, Table, Tabs, theme as antTheme,
} from 'antd';
import {
  ArrowClockwiseIcon, ArrowLeftIcon, ArrowRightIcon, ArrowUpRightIcon,
  CaretDownIcon, CaretRightIcon, ChartLineUpIcon, CheckCircleIcon, CheckIcon,
  CircleHalfIcon, ClockIcon, DatabaseIcon, DotsThreeIcon, FlaskIcon,
  IconContext, InfoIcon, ListIcon, MagnifyingGlassIcon, MoonIcon, PlugsIcon,
  PlusIcon, SignOutIcon, SlidersHorizontalIcon, SquaresFourIcon, SunIcon,
  WarningCircleIcon,
} from '@phosphor-icons/react';
import '@fontsource-variable/geist';
import '@fontsource-variable/geist-mono';
import './prototype.css';

type View = 'research' | 'new' | 'data' | 'portfolios' | 'observations' | 'settings' | 'extensions';
type Appearance = 'system' | 'light' | 'dark';
type Trial = { id: number; hypothesis: string; status: '已计算' | '失败' | '计算中' | '已拒绝'; error?: string };
type Study = { id: number; title: string; status: string; limit: number; selected: number | null; trials: Trial[] };
const params = new URLSearchParams(location.search);
const studies: Study[] = [
  {
    id: 18, title: '成交量变化能否解释短期收益？', status: '已计算', limit: 4, selected: 103,
    trials: [
      { id: 103, hypothesis: '按原政策检验成交量偏离信号', status: '已计算' },
      { id: 102, hypothesis: '比较短期价格与成交量变化', status: '失败', error: 'native_forecast_failed' },
      { id: 101, hypothesis: '检验价格变化的条件关系', status: '已计算' },
    ],
  },
  {
    id: 17, title: '价格与成交量的短期联动', status: '研究中', limit: 4, selected: null,
    trials: [
      { id: 99, hypothesis: '检验价格变化的滞后关系', status: '计算中' },
      { id: 98, hypothesis: '比较价格变化与成交量偏离', status: '已计算' },
    ],
  },
  {
    id: 16, title: '极端波动后的均值回归', status: '独立评估失败', limit: 4, selected: 95,
    trials: [
      { id: 95, hypothesis: '检验极端价格变化的后续关系', status: '已计算' },
      { id: 94, hypothesis: '比较价格区间与成交量偏离', status: '失败', error: 'native_forecast_failed' },
      { id: 93, hypothesis: '检验波动扩张后的条件关系', status: '已计算' },
      { id: 92, hypothesis: '追加一次候选试验', status: '已拒绝', error: 'trial_budget_exhausted' },
    ],
  },
];
const navigation: { view: View; label: string; icon: ReactNode; secondary?: boolean }[] = [
  { view: 'research', label: '研究', icon: <FlaskIcon /> },
  { view: 'data', label: '数据', icon: <DatabaseIcon /> },
  { view: 'portfolios', label: '组合回测', icon: <ChartLineUpIcon /> },
  { view: 'observations', label: '目标与观测', icon: <ArrowClockwiseIcon /> },
  { view: 'settings', label: '模型设置', icon: <PlugsIcon />, secondary: true },
  { view: 'extensions', label: '扩展', icon: <SquaresFourIcon />, secondary: true },
];

function Status({ children, plain = false }: { children: string; plain?: boolean }) {
  const tone = /失败|拒绝/.test(children) ? 'error' : /待核对|待重启/.test(children) ? 'warning' : /研究中|计算中/.test(children) ? 'active' : 'neutral';
  const Icon = tone === 'error' || tone === 'warning' ? WarningCircleIcon : tone === 'active' ? ClockIcon : CheckCircleIcon;
  return <span className={`status ${tone} ${plain ? 'plain' : ''}`}><Icon size={14} />{children}</span>;
}

function Facts({ items }: { items: [string, ReactNode][] }) {
  return <dl className="facts">{items.map(([label, value]) => <div key={label}><dt>{label}</dt><dd>{value}</dd></div>)}</dl>;
}

function Empty({ title, action }: { title: string; action?: ReactNode }) {
  return <div className="empty"><div className="empty-icon"><FlaskIcon size={24} weight="duotone" /></div><h3>{title}</h3>{action}</div>;
}

function Prototype() {
  const [mode, setMode] = useState<Appearance>(params.get('theme') === 'dark' ? 'dark' : params.get('theme') === 'light' ? 'light' : 'system');
  const [systemDark, setSystemDark] = useState(matchMedia('(prefers-color-scheme: dark)').matches);
  const [view, setView] = useState<View>((params.get('view') as View) || 'research');
  const [selected, setSelected] = useState<number | null>(18);
  const [tab, setTab] = useState(params.get('tab') || 'evidence');
  const [query, setQuery] = useState('');
  const [filter, setFilter] = useState('全部状态');
  const [drawer, setDrawer] = useState(false);
  const [expanded, setExpanded] = useState<number | null>(null);
  const [dialog, setDialog] = useState<string | null>(null);
  const [extensionEnabled, setExtensionEnabled] = useState(false);
  const [form] = Form.useForm();
  const dark = mode === 'dark' || mode === 'system' && systemDark;
  const study = studies.find(item => item.id === selected) ?? studies[0]!;
  const go = (next: View) => { setView(next); setDrawer(false); };
  const openTrial = (id: number) => { setTab('trials'); setExpanded(id); };

  useEffect(() => {
    const media = matchMedia('(prefers-color-scheme: dark)');
    const update = () => setSystemDark(media.matches);
    media.addEventListener('change', update);
    return () => media.removeEventListener('change', update);
  }, []);

  const colors = dark ? {
    canvas: '#171B20', surface: '#20252B', text: '#E7ECF1', secondary: '#A7B1BC',
    accent: '#94C7BC', primary: '#E7ECF1', primaryText: '#202830', border: '#70808E',
    line: '#38414A', inset: '#292F36', selected: '#2B3B3A', error: '#F0A29A',
  } : {
    canvas: '#F5F6F8', surface: '#FDFEFE', text: '#26313A', secondary: '#626F7B',
    accent: '#2A6B62', primary: '#293740', primaryText: '#FCFDFE', border: '#8795A1',
    line: '#E0E5EA', inset: '#F1F4F6', selected: '#EDF3F1', error: '#AF433E',
  };

  const navLinks = (secondary: boolean) => navigation.filter(item => Boolean(item.secondary) === secondary).map(item => (
    <a
      className={`nav-link ${(view === item.view || view === 'new' && item.view === 'research') ? 'selected' : ''}`}
      key={item.view} href={`#${item.view}`} aria-current={view === item.view ? 'page' : undefined}
      onClick={event => { event.preventDefault(); go(item.view); }}
    >{item.icon}<span>{item.label}</span></a>
  ));

  const appearance = <Dropdown trigger={['click']} menu={{
    selectedKeys: [mode], onClick: event => setMode(event.key as Appearance), items: [
      { key: 'system', label: '跟随系统', icon: <CircleHalfIcon /> },
      { key: 'light', label: '浅色', icon: <SunIcon /> },
      { key: 'dark', label: '深色', icon: <MoonIcon /> },
    ],
  }}><Button className="icon-button" type="text" aria-label="外观设置" icon={<CircleHalfIcon size={19} />} /></Dropdown>;

  const trialTable = <section className="trial-section">
    <div className="section-heading"><h3>试验记录</h3><span className="quiet">{study.trials.length} 次尝试</span></div>
    <Table<Trial>
      rowKey="id" size="small" pagination={false} dataSource={study.trials} scroll={{ x: 590 }}
      expandable={{
        columnWidth: 44, expandedRowKeys: expanded === null ? [] : [expanded],
        onExpand: (open, row) => setExpanded(open ? row.id : null),
        expandIcon: ({ expanded: open, onExpand, record }) => <Button
          type="text" className="trial-expander" aria-label={`${open ? '收起' : '展开'}试验 ${record.id}`}
          aria-expanded={open} icon={<CaretDownIcon className={open ? 'rotate' : ''} size={16} />}
          onClick={event => onExpand(record, event)}
        />,
        expandedRowRender: row => <div className="trial-detail">
          <div className="detail-overline"><code>#{row.id}</code><span>原始试验</span></div>
          <h4>{row.hypothesis}</h4>
          <Facts items={[["数据分区", '验证'], ["Pearson IC", '不可用'], ["原因", row.error ?? '分数恒定'], ["独立选择", row.id === study.selected ? '所选试验' : '未选择']]} />
        </div>,
      }}
      columns={[
        { title: '研究假设', dataIndex: 'hypothesis', width: 270, render: (value, row) => <div className="trial-name"><span>{value}</span><code>#{row.id}</code></div> },
        { title: '验证 IC', width: 100, align: 'right', render: (_value, row) => <div className="table-metric">{row.status === '已计算' ? '不可用' : '未提供'}{row.status === '已计算' && <small>分数恒定</small>}</div> },
        { title: '状态', width: 100, render: (_value, row) => <Status plain>{row.status}</Status> },
        { title: '选择', width: 80, render: (_value, row) => row.id === study.selected ? <span className="selected-trial"><CheckIcon size={13} />所选</span> : <span className="quiet">未选择</span> },
        Table.EXPAND_COLUMN,
      ]}
    />
  </section>;

  const trialPreview = <section className="trial-preview">
    <div className="section-heading"><h3>试验记录 <span className="heading-count">{study.trials.length}</span></h3><button className="text-link" onClick={() => setTab('trials')}>全部试验 <ArrowUpRightIcon size={14} /></button></div>
    <div className="preview-rows">{study.trials.slice(0, 3).map(row => <button className="preview-row" key={row.id} onClick={() => openTrial(row.id)}>
      <code>{row.id}</code><span className="preview-hypothesis">{row.hypothesis}</span>
      {row.id === study.selected && <span className="selected-mark">所选</span>}
      <Status plain>{row.status}</Status><CaretRightIcon size={14} />
    </button>)}</div>
  </section>;

  const evidence = <div className="evidence-content">
    <section className="independent-result">
      <div className="section-heading"><h3>独立评估</h3><button className="selection-link" onClick={() => openTrial(study.selected!)}>所选试验 <code>#{study.selected}</code><ArrowUpRightIcon size={14} /></button></div>
      <div className="metric-grid">
        <div className="metric-primary"><span className="metric-label">独立样本 IC</span><strong>不可用</strong><span className="metric-reason"><InfoIcon size={13} />分数恒定</span></div>
        <div className="metric-count"><span className="metric-label">有效观测</span><strong>2</strong></div>
        <div className="metric-count"><span className="metric-label">排除观测</span><strong>0</strong></div>
      </div>
      <div className="partition-row"><span className="partition-label">独立样本</span><span>分量 <code>0</code></span></div>
    </section>
    {trialPreview}
    <footer className="consumer-footer"><div className="consumer-label"><ChartLineUpIcon size={19} /><h3>组合回测</h3><span>可选</span></div><Button onClick={() => go('portfolios')}>创建组合 <ArrowRightIcon size={14} /></Button></footer>
  </div>;

  const filtered = studies.filter(row => row.title.includes(query) && (filter === '全部状态' || filter === row.status));
  const research = <div className={`research-layout ${selected !== null ? 'has-selection' : ''}`}>
    <section className="collection" aria-label="研究列表">
      <header className="collection-heading"><div>{selected === null ? <h1>研究</h1> : <h2>研究</h2>}<span>3</span></div><Button type="primary" className="new-study-button" icon={<PlusIcon size={15} />} onClick={() => go('new')}>新建研究</Button></header>
      <div className="collection-tools">
        <label className="sr-only" htmlFor="research-search">搜索已加载研究</label>
        <Input id="research-search" className="research-search" prefix={<MagnifyingGlassIcon size={16} />} allowClear value={query} onChange={event => setQuery(event.target.value)} placeholder="搜索研究" />
        <Select className="state-filter" aria-label="研究状态" value={filter} onChange={setFilter} suffixIcon={<CaretDownIcon size={12} />} options={['全部状态', '研究中', '已计算', '独立评估失败'].map(value => ({ value }))} />
      </div>
      <div className="collection-scope"><span>已加载 3 条</span>{filtered.length > 0 && (query || filter !== '全部状态') && <button className="text-link" onClick={() => { setQuery(''); setFilter('全部状态'); }}>清除筛选</button>}</div>
      <div className="study-list">{filtered.map(row => <a className={`study-row ${selected === row.id ? 'active' : ''}`} key={row.id} href={`#study-${row.id}`} aria-current={selected === row.id ? 'true' : undefined} onClick={event => { event.preventDefault(); setSelected(row.id); setTab('evidence'); setExpanded(null); }}>
        <div className="study-row-top"><code>#{row.id}</code><Status plain>{row.status}</Status></div>
        <h3>{row.title}</h3><div className="study-meta"><span>{row.trials.length} 次试验</span><span>Faux Provider</span></div>
      </a>)}</div>
      {!filtered.length && <div className="no-matches"><MagnifyingGlassIcon size={21} /><h3>没有匹配的研究</h3><Button type="text" onClick={() => { setQuery(''); setFilter('全部状态'); }}>清除筛选</Button></div>}
      <div className="collection-bottom"><DatabaseIcon size={15} /><span>1 个已登记数据集</span><button aria-label="查看数据" onClick={() => go('data')}><ArrowUpRightIcon size={15} /></button></div>
    </section>
    {selected === null ? <div className="study-document"><Empty title="选择一项研究" /></div> : <article className="study-document">
      <header className="study-heading">
        <div className="document-toolbar"><div className="document-identity"><button className="mobile-back text-link" onClick={() => setSelected(null)}><ArrowLeftIcon size={14} />研究列表</button><span className="desktop-breadcrumb">研究 <CaretRightIcon size={11} /> <code>{study.id}</code></span><Status>{study.status}</Status></div><Dropdown trigger={['click']} menu={{ items: [{ key: 'identity', label: '查看原始身份', onClick: () => setTab('inputs') }, ...(study.status === '研究中' ? [{ key: 'stop', label: '停止研究', danger: true, onClick: () => setDialog('设计原型不停止真实研究。') }] : [])] }}><Button className="icon-button" type="text" aria-label="研究操作" icon={<DotsThreeIcon size={21} />} /></Dropdown></div>
        <h1>{study.title}</h1>
        <div className="study-context"><span><DatabaseIcon size={14} />协议测试数据</span><span><PlugsIcon size={14} />Faux Provider</span><span><FlaskIcon size={14} /><code>{study.trials.length} / {study.limit}</code> 次试验</span></div>
      </header>
      <Tabs className="document-tabs" activeKey={tab} onChange={setTab} items={[
        { key: 'evidence', label: '证据', children: study.id === 17 ? <div className="evidence-content"><Empty title="尚无独立结果" action={<Button onClick={() => setTab('trials')}>查看试验</Button>} />{trialPreview}</div> : study.id === 16 ? <div className="evidence-content"><div className="inline-problem" role="alert"><WarningCircleIcon size={18} /><strong>独立评估失败</strong><button className="text-link" onClick={() => setDialog('native_forecast_failed')}>技术详情 <ArrowUpRightIcon size={13} /></button></div>{trialPreview}</div> : evidence },
        { key: 'trials', label: <span>试验 <span className="tab-count">{study.trials.length}</span></span>, children: trialTable },
        { key: 'inputs', label: '输入与执行', children: <div className="input-content"><section><h3>研究输入</h3><Facts items={[["数据集", '协议测试数据'], ["来源", '受控协议测试'], ["模型", 'Faux Provider'], ["推理强度", '关闭'], ["试验预算", `${study.limit} 次`], ["覆盖区间", '未提供']]} /></section><section><h3>最近执行记录</h3><Facts items={[["原始提交", <code>submission {study.id}</code>], ["提交状态", study.id === 17 ? '已送入执行' : '已回答'], ["原生任务", study.id === 17 ? 'running' : 'completed'], ["更新时间", '未记录']]} /></section></div> },
      ]} />
    </article>}
  </div>;

  const newStudy = <div className="authoring-page">
    <button className="text-link back-link" onClick={() => go('research')}><ArrowLeftIcon size={14} />研究</button>
    <div className="authoring-heading"><div className="authoring-symbol"><FlaskIcon weight="duotone" size={24} /></div><h1>新建研究</h1></div>
    <Form form={form} layout="vertical" onFinish={() => setDialog('这是独立设计原型，不会提交研究。')} initialValues={{ dataset: 'controlled', model: 'faux', thinking: 'off' }}>
      <Form.Item label="研究问题" name="goal" rules={[{ required: true, message: '请输入研究问题' }]}><Input.TextArea rows={4} placeholder="例如：成交量变化能否解释短期收益？" /></Form.Item>
      <Form.Item label="数据集" name="dataset"><Select suffixIcon={<CaretDownIcon size={14} />} options={[{ value: 'controlled', label: '协议测试数据' }]} /></Form.Item>
      <div className="dataset-context"><span><DatabaseIcon size={13} />受控协议测试</span><code>USD</code><span className="dataset-ready"><CheckCircleIcon size={13} />已就绪</span></div>
      <div className="form-section-heading"><SlidersHorizontalIcon size={16} /><h2>运行设置</h2></div>
      <Form.Item label="模型" name="model"><Select suffixIcon={<CaretDownIcon size={14} />} options={[{ value: 'faux', label: 'Faux Provider' }]} /></Form.Item>
      <div className="form-columns"><Form.Item label="推理强度" name="thinking"><Select suffixIcon={<CaretDownIcon size={14} />} options={[{ value: 'off', label: '关闭' }]} /></Form.Item><Form.Item label="试验预算" name="budget" rules={[{ required: true, message: '请输入试验预算' }]} extra="失败试验计入预算"><InputNumber min={0} precision={0} suffix="次" placeholder="输入次数" className="full-width" /></Form.Item></div>
      <div className="form-actions"><Button onClick={() => go('research')}>取消</Button><Button type="primary" htmlType="submit">开始研究 <ArrowRightIcon size={15} /></Button></div>
    </Form>
  </div>;

  const pageHeader = (title: string, trailing?: ReactNode) => <header className="page-heading"><h1>{title}</h1>{trailing}</header>;
  const data = <div className="index-page">{pageHeader('数据', <span className="quiet">1 个已登记数据集</span>)}<Table rowKey="id" pagination={false} scroll={{ x: 800 }} dataSource={[{ id: 'controlled', name: '协议测试数据', source: '受控协议测试', asset: 'TEST', coverage: '未提供', currency: 'USD' }]} columns={[
    { title: '数据集', dataIndex: 'name', width: 250, render: (name, row) => <div className="dataset-name"><span className="dataset-symbol"><DatabaseIcon size={19} /></span><div><strong>{name}</strong><code>{row.id}</code></div></div> },
    { title: '来源', dataIndex: 'source' }, { title: '资产', dataIndex: 'asset' }, { title: '覆盖区间', dataIndex: 'coverage', render: value => <span className="quiet">{value}</span> }, { title: '币种', dataIndex: 'currency', render: value => <code>{value}</code> },
    { title: '状态', render: () => <Status plain>已就绪</Status> }, { title: '', width: 128, render: () => <Button onClick={() => go('new')}>用于研究 <ArrowUpRightIcon size={13} /></Button> },
  ]} /></div>;

  const portfolios = <div className="authoring-page">
    <button className="text-link back-link" onClick={() => go('research')}><ArrowLeftIcon size={14} />研究</button>
    <div className="authoring-heading"><div className="authoring-symbol"><ChartLineUpIcon size={24} weight="duotone" /></div><h1>创建组合</h1></div>
    <Form layout="vertical" onFinish={() => setDialog('设计原型不运行真实回测。')} initialValues={{ name: '成交量信号组合', data: 'controlled', capital: '1000', weight: '1' }}>
      <Form.Item label="组合名称" name="name"><Input /></Form.Item><Form.Item label="数据集" name="data"><Select suffixIcon={<CaretDownIcon size={14} />} options={[{ value: 'controlled', label: '协议测试数据' }]} /></Form.Item>
      <div className="form-section-heading"><FlaskIcon size={16} /><h2>研究与权重</h2></div>
      <div className="member-row"><div><span className="selected-mark">所选研究</span><h3>成交量变化能否解释短期收益？</h3><code>#18</code></div><Form.Item label="权重" name="weight"><Input inputMode="decimal" /></Form.Item></div>
      <Form.Item label="本金" name="capital"><Input suffix={<span className="input-unit">USD</span>} inputMode="decimal" /></Form.Item>
      <div className="form-actions"><Button onClick={() => go('research')}>取消</Button><Button type="primary" htmlType="submit">运行回测 <ArrowRightIcon size={15} /></Button></div>
    </Form>
  </div>;

  const observations = <div className="observation-page">
    {pageHeader('目标与观测', <Button icon={<PlusIcon size={15} />} onClick={() => setDialog('此原型预览协议示例，不创建真实观测。')}>新建观测</Button>)}
    <article className="observation-document">
      <header className="observation-heading"><div><div className="record-context"><span className="mode-label">模拟</span><span>协议示例连接</span></div><h2>成交量信号组合</h2></div><Button type="primary" icon={<ArrowClockwiseIcon size={15} />} onClick={() => setDialog('设计原型不核对真实回执。')}>核对回执</Button></header>
      <section className="receipt-section"><div className="section-heading"><h3>目标回执</h3><Status>待核对</Status></div><Facts items={[["回执身份", <code>protocol-receipt</code>], ["到期时间", '未提供'], ["下游身份", '未提供'], ["接收时间", '未提供']]} /><button className="text-link provenance-link" onClick={() => setDialog('原型示例，没有真实下游目标。')}>来源与身份 <ArrowUpRightIcon size={13} /></button></section>
      <section className="snapshot-section"><div className="section-heading"><h3>账户快照</h3><span className="quiet">尚无快照</span></div><div className="snapshot-values"><div><span>权益 <code>USD</code></span><strong>未提供</strong></div><div><span>现金 <code>USD</code></span><strong>未提供</strong></div></div><div className="snapshot-empty"><ClockIcon size={17} /><span>尚无账户快照</span></div></section>
      <footer className="observation-footer"><h3>持仓</h3><span className="quiet">尚无快照</span><Button danger onClick={() => setDialog('仅停止本地观测，不撤回下游目标。')}>停止观测</Button></footer>
    </article>
  </div>;

  const settings = <div className="settings-page">{pageHeader('模型设置')}
    <section className="settings-group"><header><h2>模型连接</h2></header><div className="settings-group-body"><div className="provider-row"><div className="provider-symbol"><PlugsIcon size={21} /></div><div><strong>Faux Provider</strong><span>原型示例</span></div><Status plain>已配置</Status><Button onClick={() => setDialog('设计原型不更改模型凭据。')}>断开</Button></div></div></section>
    <section className="settings-group"><header><h2>研究默认值</h2></header><div className="settings-group-body"><Form layout="vertical" initialValues={{ model: 'faux', reasoning: 'off' }} onFinish={() => setDialog('设计原型不保存真实设置。')}><Form.Item label="默认模型" name="model"><Select suffixIcon={<CaretDownIcon size={14} />} options={[{ value: 'faux', label: 'Faux Provider' }]} /></Form.Item><Form.Item label="推理强度" name="reasoning"><Select suffixIcon={<CaretDownIcon size={14} />} options={[{ value: 'off', label: '关闭' }]} /></Form.Item><Button type="primary" htmlType="submit">保存设置</Button></Form></div></section>
    <section className="settings-group"><header><h2>外观</h2></header><div className="settings-group-body"><Segmented className="appearance-options" options={[{ value: 'system', label: <span><CircleHalfIcon size={16} />跟随系统</span> }, { value: 'light', label: <span><SunIcon size={16} />浅色</span> }, { value: 'dark', label: <span><MoonIcon size={16} />深色</span> }]} value={mode} onChange={value => setMode(value as Appearance)} /></div></section>
    <section className="settings-group"><header><h2>工作台用量</h2></header><div className="settings-group-body"><Facts items={[["输入 Token", '未提供'], ["输出 Token", '未提供'], ["费用", '未提供'], ["活动任务", <code>0</code>]]} /></div></section>
  </div>;

  const extensions = <div className="index-page">{pageHeader('扩展', <span className="quiet">2 个已登记扩展</span>)}<Table rowKey="name" pagination={false} scroll={{ x: 680 }} dataSource={[
    { name: 'qz.science', kind: 'science', version: '1', required: true, enabled: true, state: '已加载' },
    { name: 'test.protocol-research', kind: 'research', version: '未提供', required: false, enabled: extensionEnabled, state: extensionEnabled ? '已加载' : '待重启' },
  ]} columns={[
    { title: '扩展名称', dataIndex: 'name', width: 320, render: value => <div className="extension-name"><span className="dataset-symbol"><SquaresFourIcon size={18} /></span><code>{value}</code></div> },
    { title: '类型', dataIndex: 'kind' }, { title: '版本', dataIndex: 'version' },
    { title: '启用设置', render: (_value, row) => <div className="extension-preference"><Switch aria-label={`${row.name} 启用设置`} checked={row.enabled} disabled={row.required} onChange={setExtensionEnabled} />{row.required && <span className="quiet">必需</span>}</div> },
    { title: '当前状态', dataIndex: 'state', render: value => <Status plain>{value}</Status> },
  ]} /></div>;

  return <ConfigProvider button={{ autoInsertSpace: false }} theme={{
    algorithm: dark ? antTheme.darkAlgorithm : antTheme.defaultAlgorithm,
    token: {
      colorPrimary: colors.primary, colorTextLightSolid: colors.primaryText, colorBgBase: colors.surface,
      colorBgContainer: colors.surface, colorBgLayout: colors.canvas, colorText: colors.text,
      colorTextSecondary: colors.secondary, colorTextPlaceholder: colors.secondary, colorBorder: colors.border,
      colorBorderSecondary: colors.line, colorFillAlter: colors.inset, colorLink: colors.accent,
      colorError: colors.error, colorErrorText: colors.error, colorErrorBorder: colors.error,
      borderRadius: 7, controlHeight: 38, fontSize: 14, fontWeightStrong: 600,
      fontFamily: '"Geist Variable", "Noto Sans CJK SC", "Microsoft YaHei", system-ui, sans-serif', motion: false,
    },
    components: {
      Button: { primaryShadow: 'none', defaultShadow: 'none', dangerShadow: 'none', fontWeight: 500 },
      Table: { headerBg: colors.surface, headerColor: colors.secondary, headerSplitColor: 'transparent', borderColor: colors.line, cellPaddingBlock: 15, rowHoverBg: colors.inset, rowExpandedBg: colors.inset },
      Tabs: { horizontalItemGutter: 28, inkBarColor: colors.accent, itemSelectedColor: colors.text, itemActiveColor: colors.accent, titleFontSize: 13 },
      Select: { optionSelectedBg: colors.selected, optionSelectedColor: colors.text },
      Segmented: { itemSelectedBg: colors.surface },
    },
  }}><App><IconContext.Provider value={{ size: 17, weight: 'regular', 'aria-hidden': true }}>
    <div className="prototype" data-theme={dark ? 'dark' : 'light'}>
      <div className="prototype-label"><span>界面设计预览</span><span>示例数据，不连接服务</span></div>
      <a href="#main-content" className="skip-link">跳到内容</a>
      <header className="app-topbar"><div className="topbar-inner">
        <a className="wordmark" href="#research" onClick={event => { event.preventDefault(); go('research'); }}>QuaZonai</a>
        <nav className="primary-nav" aria-label="主要工作区">{navLinks(false)}</nav>
        <div className="topbar-right"><nav className="secondary-nav" aria-label="工作台设置">{navLinks(true)}</nav><div className="topbar-divider" />{appearance}<Button className="icon-button logout-button" type="text" aria-label="退出登录" icon={<SignOutIcon size={19} />} onClick={() => setDialog('设计原型不连接真实会话。')} /></div>
        <Button className="mobile-menu icon-button" type="text" icon={<ListIcon size={21} />} aria-label="打开导航" onClick={() => setDrawer(true)} />
      </div></header>
      <Drawer title="QuaZonai" placement="left" getContainer={false} open={drawer} onClose={() => setDrawer(false)} width={288}><nav className="drawer-nav" aria-label="工作区">{navLinks(false)}<div className="drawer-divider" />{navLinks(true)}</nav><div className="drawer-appearance">{appearance}<span>外观设置</span></div></Drawer>
      <main id="main-content" className={view === 'research' ? 'research-main' : ''}>
        {view === 'research' ? research : view === 'new' ? newStudy : view === 'data' ? data : view === 'portfolios' ? portfolios : view === 'observations' ? observations : view === 'settings' ? settings : extensions}
      </main>
      <Modal getContainer={false} open={dialog !== null} title="设计预览" onCancel={() => setDialog(null)} footer={<Button onClick={() => setDialog(null)}>关闭</Button>}><p>{dialog}</p></Modal>
    </div>
  </IconContext.Provider></App></ConfigProvider>;
}

createRoot(document.getElementById('root')!).render(<Prototype />);
