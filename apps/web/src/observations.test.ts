import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { expect, it, vi } from 'vitest';
import Observations from './observations';
vi.mock('@tanstack/react-query',()=>({useQueryClient:()=>({})}));
vi.mock('./commands',()=>({draftIsPersisted:()=>true,readDraft:()=>({purpose:'TARGET_DELIVERY',portfolioId:'retired'}),removeDraft:()=>{},saveDraft:()=>true,useCommand:()=>({submit:()=>{},busy:false})}));
vi.mock('./data-hooks',()=>({useResource:()=>({isPending:false,data:{connections:[{id:'paper',name:'Paper',mode:'paper'}]}})}));
vi.mock('./account-controls',()=>({CapitalControls:()=>null}));
vi.mock('./account-strategy',()=>({default:()=>null}));
vi.mock('./ui',async importOriginal=>({...await importOriginal<typeof import('./ui')>(),useConfirm:()=>()=>{}}));
it('account binding never exposes retired target delivery even from an old portfolio link or draft',()=>{
 const html=renderToStaticMarkup(createElement(Observations,{route:{page:'observations',creating:true,params:new URLSearchParams('portfolio=old')}}));
 expect(html).toContain('绑定已有账户');expect(html).toContain('不启动策略');
 expect(html).not.toContain('交付并观测');expect(html).not.toContain('按账户净值自动调整');expect(html).not.toContain('交付已有组合目标');
});

import { Exact } from './ui';
it('missing position values display as not provided rather than zero',()=>{
 const html=renderToStaticMarkup(createElement(Exact,{value:null}));
 expect(html).toContain('未提供');expect(html).not.toContain('>0<');
});
