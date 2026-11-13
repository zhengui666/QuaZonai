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
