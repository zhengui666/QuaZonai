import { Component, type ReactNode } from 'react';
import { Button, ConfigProvider, Popconfirm, Result, theme } from 'antd';
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
          extra={
            <Popconfirm
              title="确认重新加载页面？"
              description="未保存内容将丢失；已提交操作不会撤销。"
              okText="确认重新加载"
              cancelText="留在此页"
              onConfirm={() => window.location.reload()}
            >
              <Button type="primary">重新加载页面</Button>
            </Popconfirm>
          }
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
