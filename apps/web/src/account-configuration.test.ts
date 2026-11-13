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
