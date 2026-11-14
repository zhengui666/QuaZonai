import {it,expect} from 'vitest';
import {createElement} from 'react';
import {renderToStaticMarkup} from 'react-dom/server';
import {PreparationStatus,SelectedRunnerStatus,preparationStatus} from './native-preparation-status';
it('preserves terminal unknown as uncertainty rather than success or definite non-execution',()=>{
 const value={requestId:'original-request',taskId:42,state:{status:'terminal',outcome:{status:'failed',error:{message:'native_build_original_outcome_unknown_no_automatic_retry'}}}};
 const html=renderToStaticMarkup(createElement(PreparationStatus,{value}));expect(html).toContain('原准备结果未知');expect(html).toContain('original-request');expect(html).toContain('42');expect(html).toContain('不自动重编');expect(html).not.toContain('<pre');
 expect(preparationStatus({...value,state:{status:'running'}}).label).toBe('准备任务尚未结束');
 expect(preparationStatus({...value,state:{status:'terminal',outcome:{status:'completed'}}}).label).toBe('产物准备已核实');
 expect(preparationStatus({...value,state:{status:'terminal',outcome:{status:'aborted'}}}).detail).toContain('不证明');
 expect(preparationStatus({...value,state:{status:'terminal',outcome:{status:'failed',error:{message:'private diagnostic'}}}}).detail).toContain('不推断');
});
it('shows saved selection separately without executable paths or arbitrary identity diagnostics',()=>{
 const runner={kind:'RUST_PROCESS',id:'owner',binary:'/private/runner',capabilities:{identity:{nautilusVersion:'0.69.0',precision:'high',nautilusSource:'path+file:///private/source',private:'secret'},account:{compiled:true,registration:false},research:{configuredRunFactory:true}}} as any;
 const html=renderToStaticMarkup(createElement(SelectedRunnerStatus,{runner}));expect(html).toContain('0.69.0');expect(html).toContain('high');expect(html).toContain('不表示上方每个历史准备请求');expect(html).toContain('旧账户不自动改绑');expect(html).not.toContain('/private');expect(html).not.toContain('secret');expect(html).not.toContain('<pre');
});
