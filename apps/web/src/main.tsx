import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import App from './App';
import 'antd/dist/reset.css';
import './styles.css';

// Retire the previous PWA worker; authenticated requests stay network-only.
if ('serviceWorker' in navigator) {
  void navigator.serviceWorker.getRegistrations().then(registrations => Promise.all(registrations.map(registration => registration.unregister())));
}
const root = document.getElementById('root');
if (!root) throw new Error('Application root is missing');
createRoot(root, { onCaughtError: () => { console.error('QuaZonai: page unavailable'); } }).render(<StrictMode><App /></StrictMode>);
