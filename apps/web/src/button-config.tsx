import { LoadingOutlined } from '@ant-design/icons';
import type { ConfigProviderProps } from 'antd';

// A decorative loading glyph must not become part of any button's accessible
// name, including while a fast request's loading transition is leaving.
export const buttonConfig: NonNullable<ConfigProviderProps['button']> = {
  autoInsertSpace: false,
  loadingIcon: <LoadingOutlined aria-hidden />,
};
