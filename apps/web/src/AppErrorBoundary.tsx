import { Component, type ReactNode } from 'react';
import { Button, ConfigProvider, Popconfirm, Result, theme } from 'antd';
import { useColorTheme } from './theme';

type Props = { children: ReactNode; contained?: boolean };
type State = { failed: boolean };

function Recovery({ contained = false }: { contained?: boolean }) {
  const [colorTheme] = useColorTheme();
  const dark = colorTheme === 'dark';
  const Container = contained ? 'section' : 'main';
  return (
    <ConfigProvider theme={{
      algorithm: dark ? theme.darkAlgorithm : theme.defaultAlgorithm,
      token: { colorPrimary: '#2857b4', motion: false },
    }}>
      <Container style={{ maxWidth: 640, margin: '48px auto', padding: 24 }}>
        <Result
          status="error"
          title={<h1 style={{ fontSize: 24 }}>页面暂时无法显示</h1>}
          subTitle={contained ? '可以从主导航切换到其他页面，或重新加载。' : undefined}
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
      </Container>
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
    return this.state.failed ? <Recovery contained={this.props.contained} /> : this.props.children;
  }
}
