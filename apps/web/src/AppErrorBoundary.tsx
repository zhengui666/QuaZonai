import { Component, type ReactNode } from 'react';
import { Button, ConfigProvider, Popconfirm, Result, theme } from 'antd';
import { useColorTheme } from './theme';
import { useSettingsWork } from './settings-work';
import { protectedWorkActive } from './ui';

type Props = { children: ReactNode; contained?: boolean };
type State = { failed: boolean; protectedAtFailure: boolean };

function Recovery({ contained = false, protectedAtFailure }: { contained?: boolean; protectedAtFailure: boolean }) {
  const [colorTheme] = useColorTheme();
  const recoverableWork = useSettingsWork() || protectedAtFailure;
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
          extra={recoverableWork ? <Popconfirm title="操作内容可能丢失" description="重新加载会清除本页输入、重试身份或回执。若刚提交请求，请先核对结果。"
            okText="确认重新加载" cancelText="留在此页" onConfirm={() => window.location.reload()}>
            <Button type="primary">重新加载页面</Button>
          </Popconfirm> : <Button type="primary" onClick={() => window.location.reload()}>重新加载页面</Button>}
        />
      </Container>
    </ConfigProvider>
  );
}

/** Render recovery only: never retry commands or expose exception contents. */
export default class AppErrorBoundary extends Component<Props, State> {
  override state: State = { failed: false, protectedAtFailure: false };

  static getDerivedStateFromError(): State {
    return { failed: true, protectedAtFailure: protectedWorkActive() };
  }

  override render() {
    return this.state.failed ? <Recovery contained={this.props.contained} protectedAtFailure={this.state.protectedAtFailure} /> : this.props.children;
  }
}
