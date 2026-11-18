import { tr } from './i18n';
import { Component, type ReactNode } from 'react';
import { Button, Result } from 'antd';

export default class AppErrorBoundary extends Component<{ children: ReactNode }, { failed: boolean }> {
  override state = { failed: false };
  static getDerivedStateFromError() { return { failed: true }; }
  override render() {
    return this.state.failed ? <Result status="error" title={tr("Page unavailable")} extra={<Button onClick={() => window.location.reload()}>{tr("Reload")}</Button>} /> : this.props.children;
  }
}
