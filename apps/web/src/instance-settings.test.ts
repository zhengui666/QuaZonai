import {describe,it,expect} from 'vitest';
import {createElement} from 'react';
import {renderToStaticMarkup} from 'react-dom/server';
import {environmentFromFields,AccountCompatibilityNotice,retryUncertainEnvironmentSave} from './instance-settings';
describe('environment configuration retry semantics',()=>{
 it('unlocks definitive invalid/stale inputs but retains an uncertain request',()=>{
  expect(retryUncertainEnvironmentSave(400)).toBe(false);
  expect(retryUncertainEnvironmentSave(409)).toBe(false);
  expect(retryUncertainEnvironmentSave(422)).toBe(false);
  expect(retryUncertainEnvironmentSave(503)).toBe(true);
  expect(retryUncertainEnvironmentSave()).toBe(true);
 });
});

describe('explicit trusted account compatibility form',()=>{
 const fields={id:'owner',cargoBinary:'/cargo',manifestPath:'/owner/Cargo.toml',packageName:'owner',accountFactory:' account ',featureNames:'high-precision'};
 it('does not opt in by finding an account factory and preserves explicit saved declaration on edit',()=>{
  expect(environmentFromFields(fields)).not.toHaveProperty('accountCompatibility');
  const saved=environmentFromFields({...fields,accountCompatibility:'TRUSTED_STRATEGY_ONLY_SINGLE_NODE_V1'});
  expect(saved.accountCompatibility).toBe('TRUSTED_STRATEGY_ONLY_SINGLE_NODE_V1');
  expect(environmentFromFields({...saved,featureNames:saved.features?.join(','),id:'edited'}).accountCompatibility).toBe(saved.accountCompatibility);
  expect(environmentFromFields({...fields,accountCompatibility:undefined})).not.toHaveProperty('accountCompatibility');
 });
 it('rejects an orphan or unknown declaration instead of silently dropping or asserting compatibility',()=>{
  expect(()=>environmentFromFields({...fields,accountFactory:' ',accountCompatibility:'TRUSTED_STRATEGY_ONLY_SINGLE_NODE_V1'})).toThrow('需要账户策略工厂');
  expect(()=>environmentFromFields({...fields,accountCompatibility:'certified' as never})).toThrow();
 });
 it('separates an artifact declaration from observed account capability and avoids certification claims',()=>{
  const declared=renderToStaticMarkup(createElement(AccountCompatibilityNotice,{declaration:'TRUSTED_STRATEGY_ONLY_SINGLE_NODE_V1'}));
  expect(declared).toContain('自报已声明');expect(declared).toContain('不是认证');expect(declared).toContain('不证明账户取消当前可用');expect(declared).toContain('清除当前产物选择');expect(declared).toContain('重新显式选择');expect(declared).toContain('不会修改产物');
  const missing=renderToStaticMarkup(createElement(AccountCompatibilityNotice,{declaration:undefined}));expect(missing).toContain('未报告');expect(missing).toContain('不会据保存配置补充');
 });
});
