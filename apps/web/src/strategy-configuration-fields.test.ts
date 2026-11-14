import {it,expect} from 'vitest';
import {createElement} from 'react';
import {renderToStaticMarkup} from 'react-dom/server';
import {strategyConfigurationField,StrategyConfigurationFields} from './account-strategy';
it('edits only original deployment strings without rebuilding opaque owner parameters',()=>{
 const original={instrument_id:'OLD.SIM',future:['original'],strategy:{kind:'REGISTERED',name:'original',config:{size:'9007199254740993.00001',ns:'18446744073709551615',nested:[null,{unknown:true}]},future:'untouched'}};
 let text=JSON.stringify(original);
 for(const name of ['o','owner','owner::original'])text=strategyConfigurationField(text,'name',name);
 const edited=JSON.parse(strategyConfigurationField(text,'instrument_id','NEW.SIM'));
 expect(edited).toEqual({...original,instrument_id:'NEW.SIM',strategy:{...original.strategy,name:'owner::original'}});
 expect(JSON.parse(strategyConfigurationField(text,'name','')).strategy.name).toBe('');
 for(const invalid of ['null','[]','{'])expect(()=>strategyConfigurationField(invalid,'instrument_id','x')).toThrow();
 expect(()=>strategyConfigurationField('{"strategy":{"kind":"EMA_CROSS"}}','name','owner')).toThrow(/不替换/);
});
it('shows known original fields with original disabled state, never invents factory schema',()=>{
 let edits=0;const render=(text:string,disabled:boolean)=>renderToStaticMarkup(createElement(StrategyConfigurationFields,{text,disabled,onChange:()=>edits++}));
 const text=JSON.stringify({instrument_id:'TEST.SIM',strategy:{kind:'REGISTERED',name:'owner',config:{}}});
 const html=render(text,true);expect(html).toContain('编辑策略标的');expect(html).toContain('编辑原策略名称');expect(html).toContain('disabled');expect(html).toContain('没有可用 schema');expect(edits).toBe(0);
 expect(render('{',false)).toContain('不生成替代配置');expect(render('{"strategy":{"kind":"EMA_CROSS"}}',false)).not.toContain('编辑原策略名称');
});
