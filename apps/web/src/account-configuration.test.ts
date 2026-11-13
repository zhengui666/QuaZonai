import {describe,it,expect} from 'vitest';
import {accountConfigurationInput} from './account-configuration';
describe('explicit account configuration input',()=>{
 it('preserves original registered configuration JSON without converting decimal strings',()=>{
  const input={id:'owner-paper',mode:'paper',nativeStrategy:{instrument_id:'TEST.SIM',strategy:{kind:'REGISTERED',name:'owner::strategy',config:[null,'9007199254740993.00001']}}};
  expect(accountConfigurationInput(JSON.stringify(input))).toEqual(input);
 });
 it('refuses absent and non-object configuration rather than inventing an account',()=>{
  for(const text of ['', 'null','[]','1'])expect(()=>accountConfigurationInput(text)).toThrow();
 });
});

import {paperConfigurationField} from './account-configuration';
describe('official paper parameter editor',()=>{
 it('retains seed/nanosecond strings and unrelated strategy configuration exactly',()=>{
  const original={mode:'paper',nativeStrategy:{strategy:{kind:'REGISTERED',config:['0.0000000000000000001']}},paperExecution:{random_seed:'18446744073709551615',base_latency_ns:'9007199254740993'}};
  const edited=accountConfigurationInput(paperConfigurationField(JSON.stringify(original),'cancel_latency_ns','9007199254740995'));
  expect(edited.nativeStrategy).toEqual(original.nativeStrategy);
  expect(edited.paperExecution).toEqual({...original.paperExecution,cancel_latency_ns:'9007199254740995'});
  expect(accountConfigurationInput(paperConfigurationField(JSON.stringify(edited),'prob_slippage','0.125')).paperExecution).toEqual({...edited.paperExecution as object,prob_slippage:0.125});
 });
 it('does not invent paper parameters and refuses live edits',()=>{
  expect(accountConfigurationInput('{"mode":"paper"}')).not.toHaveProperty('paperExecution');
  expect(()=>paperConfigurationField('{"mode":"live"}','random_seed','1')).toThrow(/paper/);
  expect(()=>paperConfigurationField('{"mode":"paper"}','unsupported','1')).toThrow();
 });
});

import {PaperProbabilityInput} from './account-configuration';
it('retains decimal keystrokes until blur, then synchronizes numeric JSON',()=>{
 let json='{"mode":"paper","paperExecution":{"prob_slippage":0}}';
 const input=PaperProbabilityInput({label:'probability',value:0,disabled:false,onCommit:text=>{json=paperConfigurationField(json,'prob_slippage',text);}});
 expect(input.props.value).toBeUndefined();expect(input.props.onChange).toBeUndefined();
 let draft='';
 for(const typed of ['0','0.','0.1','0.12','0.125']){draft=typed;expect(JSON.parse(json).paperExecution.prob_slippage).toBe(0);}
 input.props.onBlur({target:{value:draft}});
 expect(JSON.parse(json).paperExecution.prob_slippage).toBe(0.125);
 const synchronized=PaperProbabilityInput({label:'probability',value:0.125,disabled:false,onCommit:()=>{}});
 expect(synchronized.props.defaultValue).toBe('0.125');expect(synchronized.key).not.toBe(input.key);
});

it('invalid probability blur explicitly restores the original value; correction and Enter commit remain possible',()=>{
 let json='{"mode":"paper","paperExecution":{"prob_slippage":0.25}}',error='';
 const input=PaperProbabilityInput({label:'probability',value:0.25,disabled:false,onCommit:text=>{
  try{json=paperConfigurationField(json,'prob_slippage',text);error='';return true;}catch{error='未写入配置，输入已恢复为原 JSON 值。';return false;}
 }});
 const target={value:'abc'};input.props.onBlur({target});
 expect(target.value).toBe('0.25');expect(error).toContain('恢复');
 expect(accountConfigurationInput(json).paperExecution).toEqual({prob_slippage:0.25});
 target.value='0.125';let blurred=false;
 input.props.onPressEnter({currentTarget:{blur(){blurred=true;input.props.onBlur({target});}}});
 expect(blurred).toBe(true);expect(error).toBe('');
 expect(accountConfigurationInput(json).paperExecution).toEqual({prob_slippage:0.125});
});


import {accountConfigurationField,AccountFields} from './account-configuration';
import {createElement} from 'react';
import {renderToStaticMarkup} from 'react-dom/server';
it('structured fields preserve exact identities, decimals and opaque factory parameters',()=>{
 const original={mode:'paper',nativeStrategy:{instrument_id:'TEST.SIM',strategy:{kind:'REGISTERED',name:'owner',config:[null,'9007199254740993.00001']}},risk:{original:'0.01'},paperExecution:{random_seed:'18446744073709551615'}};
 let text=JSON.stringify(original);
 for(const [key,value] of [['id','paper-one'],['clientId','original-client'],['accountId','SIM-001'],['instruments','TEST.SIM\nOTHER.SIM'],['startingBalance','9007199254740993.00001'],['paperFeeRate','0.000000000000000001'],['name','owner::actual']])text=accountConfigurationField(text,key!,value!);
 const result=accountConfigurationInput(text);
 expect(result.startingBalance).toBe('9007199254740993.00001');expect(result.paperFeeRate).toBe('0.000000000000000001');expect(result.instruments).toEqual(['TEST.SIM','OTHER.SIM']);
 expect(result.nativeStrategy).toEqual({...original.nativeStrategy,strategy:{...original.nativeStrategy.strategy,name:'owner::actual'}});expect(result.risk).toEqual(original.risk);expect(result.paperExecution).toEqual(original.paperExecution);
 expect(accountConfigurationInput(accountConfigurationField(text,'mode','live')).startingBalance).toBe(result.startingBalance);
 expect(()=>accountConfigurationField(text,'unsupported','x')).toThrow();
});
it('renders ordinary account fields without inventing factory schema or submitting',()=>{
 let edits=0;const html=renderToStaticMarkup(createElement(AccountFields,{text:JSON.stringify({mode:'paper',nativeStrategy:{strategy:{kind:'REGISTERED'}}}),onChange:()=>edits++,disabled:false}));
 expect(html).toContain('本地账户名称');expect(html).toContain('模拟初始余额');expect(html).toContain('没有可用 schema');expect(html).toContain('已编入运行器的策略名称');expect(edits).toBe(0);
 expect(renderToStaticMarkup(createElement(AccountFields,{text:'{',onChange:()=>edits++,disabled:false}))).toContain('不会保存替代值');
});

import {AccountInstrumentsInput,accountConfigurationForSave} from './account-configuration';
it('keeps each instrument newline while editing and parses only the explicit save payload',()=>{
 const text=JSON.stringify({mode:'paper',instruments:['OLD.SIM'],opaque:{decimal:'9007199254740993.00001'}});
 let draft:string|undefined;
 for(const value of ['T','TEST','TEST\n','TEST\nO','TEST\nOTHER','TEST\nOTHER\n']){
  const input=AccountInstrumentsInput({value:draft??'OLD.SIM',onChange:value=>{draft=value;},disabled:false});
  input.props.onChange({target:{value}});
  const rendered=AccountInstrumentsInput({value:draft!,onChange:()=>{},disabled:false});
  expect(rendered.props.value).toBe(value);
  expect(accountConfigurationInput(text).instruments).toEqual(['OLD.SIM']);
 }
 const edited=accountConfigurationField(text,'id','new-account');
 expect(accountConfigurationForSave(edited,draft)).toEqual({mode:'paper',id:'new-account',instruments:['TEST','OTHER'],opaque:{decimal:'9007199254740993.00001'}});
 // Clearing the draft after an explicit advanced-JSON edit uses that original value.
 expect(accountConfigurationForSave(text,undefined).instruments).toEqual(['OLD.SIM']);
 expect(accountConfigurationForSave(text,'').instruments).toEqual([]);
});
