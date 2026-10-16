import { Component, type ReactNode } from 'react';
import { Button, Result } from 'antd';

export default class AppErrorBoundary extends Component<{ children: ReactNode }, { failed: boolean }> {
  override state = { failed: false };
  static getDerivedStateFromError() { return { failed: true }; }
  override render() {
    return this.state.failed ? <Result status="error" title="页面不可用" extra={<Button onClick={() => window.location.reload()}>重新加载</Button>} /> : this.props.children;
  }
}
