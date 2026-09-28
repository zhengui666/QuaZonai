import { Component, type ReactNode } from 'react';
import { Button, ConfigProvider, Result, theme } from 'antd';
import { useColorTheme } from './theme';

type Props = { children: ReactNode };
type State = { failed: boolean };

function Recovery() {
  const [colorTheme] = useColorTheme();
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
          extra={<Button type="primary" onClick={() => window.location.reload()}>重新加载页面</Button>}
        />
      </main>
    </ConfigProvider>
  );
}

/** Render recovery only: never retry commands or expose exception contents. */
export default class AppErrorBoundary extends Component<Props, State> {
  override state: State = { failed: false };

  static getDerivedStateFromError(): State {
    return { failed: true };
  }

  override render() {
    return this.state.failed ? <Recovery /> : this.props.children;
  }
}
