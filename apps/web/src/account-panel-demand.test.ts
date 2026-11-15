import {describe,it,expect,vi} from 'vitest';
const state=vi.hoisted(()=>({values:[false,false],cursor:0}));
vi.mock('react',async importOriginal=>({...await importOriginal<typeof import('react')>(),useState:(initial:unknown)=>{const index=state.cursor++;return [state.values[index]??initial,(value:boolean)=>{state.values[index]=value;}];}}));
vi.mock('@tanstack/react-query',async importOriginal=>({...await importOriginal<typeof import('@tanstack/react-query')>(),useIsFetching:()=>0}));
import {SavedAccountPanel} from './account-configuration';
import {ResourceActivity} from './data-hooks';
it('does not mount closed accounts; preserves the same subtree and drafts when collapsed after use',()=>{
 const item={id:'paper-original',accountConfiguration:{mode:'paper' as const}};
 const render=()=>{state.cursor=0;return SavedAccountPanel({item});};
 state.values=[false,false];let panel=render();expect(panel.props.children[1]).toBe(false);
 panel.props.onToggle({currentTarget:{open:true}});panel=render();
 const first=panel.props.children[1];expect(first.type).toBe(ResourceActivity.Provider);expect(first.props.value).toBe(true);
 panel.props.onToggle({currentTarget:{open:false}});panel=render();
 const hidden=panel.props.children[1];expect(hidden.type).toBe(first.type);expect(hidden.props.value).toBe(false);
 expect(hidden.props.children.at(-1).type).toBe(first.props.children.at(-1).type);
 expect(hidden.props.children.at(-1).props.value.id).toBe('paper-original');
 panel.props.onToggle({currentTarget:{open:true}});expect(render().props.children[1].props.value).toBe(true);
});
