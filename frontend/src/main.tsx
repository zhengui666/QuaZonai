import '@radix-ui/themes/styles.css';
import './styles/theme.css';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { BrowserRouter } from 'react-router-dom';
import { App } from './app/App';
const queryClient=new QueryClient({defaultOptions:{queries:{staleTime:5_000,retry:(count,error)=>{const status=(error as {status?:number}).status;return status&&status>=400&&status<500?false:count<2},refetchOnWindowFocus:false},mutations:{retry:0}}});
createRoot(document.getElementById('root')!).render(<StrictMode><QueryClientProvider client={queryClient}><BrowserRouter><App/></BrowserRouter></QueryClientProvider></StrictMode>);
