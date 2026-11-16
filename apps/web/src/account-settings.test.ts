import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { expect, it, vi } from 'vitest';
const state = vi.hoisted(() => ({ compiled: false, success: true, paths: [] as string[], enabled: [] as boolean[] }));
vi.mock('./data-hooks', () => ({useResource: (path: string) => {state.paths.push(path);return {isSuccess:state.success,isPending:false,error:null,data:{runner:{capabilities:{account:{compiled:state.compiled}}}}};}}));
vi.mock('./account-configuration', () => ({default: ({enabled}:{enabled:boolean}) => {state.enabled.push(enabled);return createElement('div',null,'existing account configuration');}}));
vi.mock('./ui', () => ({ErrorNotice:()=>null,Loading:()=>null}));
import AccountSettings from './account-settings';
it('exposes the existing account form without connecting accounts or accepting keys', () => {
  state.paths=[];state.enabled=[];state.compiled=true;state.success=true;
  const html=renderToStaticMarkup(createElement(AccountSettings));
  expect(state.paths).toEqual(['/settings/instances']);
  expect(state.enabled).toEqual([true]);
  expect(html).toContain('existing account configuration');
  expect(html).toContain('此处不接收私钥');
  expect(html).toContain('/settings?section=instances');
});
it('keeps creation disabled without verified account capability while preserving the form', () => {
  for(const [compiled,success] of [[false,true],[true,false]] as const){
    state.compiled=compiled;state.success=success;state.enabled=[];
    expect(renderToStaticMarkup(createElement(AccountSettings))).toContain('existing account configuration');
    expect(state.enabled).toEqual([false]);
  }
});

import {ProviderAccountConfiguration} from './account-settings';
it('unsupported official broker selections never render the Hyperliquid creation form',()=>{
 for(const provider of ['INTERACTIVE_BROKERS','COINBASE'] as const){
  state.enabled=[];
  const html=renderToStaticMarkup(createElement(ProviderAccountConfiguration,{provider,enabled:true}));
  expect(html).toContain('Nautilus 官方适配器已存在');
  expect(html).toContain('尚未接入');
  expect(html).not.toContain('existing account configuration');
  expect(state.enabled).toEqual([]);
 }
});
