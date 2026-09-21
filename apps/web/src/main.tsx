import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import App from './App';
import AppErrorBoundary from './AppErrorBoundary';
import { ColorThemeContext, useColorThemeState } from './theme';
import 'antd/dist/reset.css';
import './styles.css';
function Root() {
  const colorTheme = useColorThemeState();
  return <ColorThemeContext.Provider value={colorTheme}>
    <AppErrorBoundary><App /></AppErrorBoundary>
  </ColorThemeContext.Provider>;
}

const root = document.getElementById('root');
if (!root) throw new Error('Application root is missing');
createRoot(root, {
  // React otherwise prints caught exceptions, which can contain private data.
  onCaughtError: () => { console.error('QuaZonai: rendering failed.'); },
}).render(<StrictMode><Root /></StrictMode>);
