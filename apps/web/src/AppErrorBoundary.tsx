import { Component, type ReactNode } from 'react';
import { Button, ConfigProvider, Popconfirm, Result, theme } from 'antd';
import { useColorTheme } from './theme';
import { useSettingsWork } from './settings-work';
import { guardWorkActive } from './ui';

type Props = { children: ReactNode };
type State = { failed: boolean; protectedAtFailure: boolean };

function Recovery({ protectedAtFailure }: { protectedAtFailure: boolean }) {
  const [colorTheme] = useColorTheme();
  const recoverableWork = useSettingsWork() || protectedAtFailure;
  const dark = colorTheme === 'dark';
  return (
    <ConfigProvider theme={{
      algorithm: dark ? theme.darkAlgorithm : theme.defaultAlgorithm,
      token: { colorPrimary: '#2857b4', motion: false },
    }}>
      <main style={{ maxWidth: 640, margin: '48px auto', padding: 24 }}>
        <Result
          status="error"
          title={<h1 style={{ fontSize: 24 }}>页面暂时无法显示</h1>}
          extra={recoverableWork ? <Popconfirm title="操作内容可能丢失" description="重新加载会清除本页输入、重试身份或回执。若刚提交请求，请先核对结果。"
            okText="确认重新加载" cancelText="留在此页" onConfirm={() => window.location.reload()}>
            <Button type="primary">重新加载页面</Button>
          </Popconfirm> : <Button type="primary" onClick={() => window.location.reload()}>重新加载页面</Button>}
        />
      </main>
    </ConfigProvider>
  );
}

/** Render recovery only: never retry commands or expose exception contents. */
export default class AppErrorBoundary extends Component<Props, State> {
  override state: State = { failed: false, protectedAtFailure: false };

  static getDerivedStateFromError(): State {
    return { failed: true, protectedAtFailure: guardWorkActive() };
  }

  override render() {
    return this.state.failed ? <Recovery protectedAtFailure={this.state.protectedAtFailure} /> : this.props.children;
  }
}
