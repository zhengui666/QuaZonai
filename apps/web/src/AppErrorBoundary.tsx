import { Component, type ReactNode } from 'react';
import { Button, Popconfirm, Result } from 'antd';

type Props = { children: ReactNode };
type State = { failed: boolean };

/** Render recovery only: never retry commands or expose exception contents. */
export default class AppErrorBoundary extends Component<Props, State> {
  override state: State = { failed: false };

  static getDerivedStateFromError(): State {
    return { failed: true };
  }

  override render() {
    if (!this.state.failed) return this.props.children;
    return (
      <main style={{ maxWidth: 640, margin: '48px auto', padding: 24 }}>
        <Result
          status="error"
          title={<h1 style={{ fontSize: 24 }}>页面暂时无法显示</h1>}
          subTitle="界面发生异常，未提交的内容可能未保存。已经发送的操作可能仍在后台运行；重新加载不会取消或重新提交这些操作。恢复后请先查看原运行或记录的状态，不要直接重复创建。"
          extra={
            <Popconfirm
              title="确认重新加载页面？"
              description="未保存的内容将无法恢复。已经发送的操作不会被撤销。"
              okText="确认重新加载"
              cancelText="留在此页"
              onConfirm={() => window.location.reload()}
            >
              <Button type="primary">重新加载页面</Button>
            </Popconfirm>
          }
        />
      </main>
    );
  }
}
