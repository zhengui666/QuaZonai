import type {ProcessNativeTarget} from '../../../packages/contracts/src/nautilus.ts';
export type PreparationView={requestId:string;taskId:number;state:{status:string;outcome?:{status:string;result?:{qualification:string;target:{capabilities:unknown}};error?:{message:string}}}};
export function preparationStatus(value:PreparationView){
 const outcome=value.state.outcome;
 if(outcome?.error?.message==='native_build_original_outcome_unknown_no_automatic_retry')return {label:'原准备结果未知',detail:'原任务已结束，但原产物准备结果未知。保留原请求，只核实原证据，不自动重编或重新执行导入。'};
 if(value.state.status!=='terminal')return {label:'准备任务尚未结束',detail:'这是最近读取的原任务状态，不代表持续运行或已有可选用产物。可只读核实原进度。'};
 if(outcome?.status==='completed')return {label:'产物准备已核实',detail:'构建或导入的能力查询已完成，尚须明确选用；不等于连接账户、策略兼容认证或交易许可。'};
 if(outcome?.status==='aborted')return {label:'准备任务已停止',detail:'任务停止不证明任意用户代码及其后代进程已退出，也不证明没有外部副作用。保留原请求核对。'};
 return {label:'产物尚未核实',detail:'不能将此结果当作可用产物。核对原准备记录与私有诊断，不推断没有执行或没有副作用。'};
}
export function PreparationStatus({value}:{value:PreparationView}){
 const status=preparationStatus(value);
 return <section aria-label="原产物准备状态"><h4>{status.label}</h4><p>{status.detail}</p><dl><dt>原请求</dt><dd>{value.requestId}</dd><dt>原任务</dt><dd>{value.taskId}</dd></dl><p>准备、选用、账户连接和策略启动是独立步骤；本报告不触发下一步。</p></section>;
}
export function SelectedRunnerStatus({runner}:{runner:ProcessNativeTarget}){
 const raw=runner.capabilities.identity;
 const identity=raw&&typeof raw==='object'&&!Array.isArray(raw)?raw:{};
 const version=typeof identity.nautilusVersion==='string'&&/^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/.test(identity.nautilusVersion)&&identity.nautilusVersion.length<=80?identity.nautilusVersion:null;
 const precision=identity.precision==='high'||identity.precision==='standard'?identity.precision:null;
 return <section aria-label="当前明确选用的产物"><h4>已明确选用一个产物</h4><p>这不表示上方每个历史准备请求都是当前选择；来源为最近读取的已保存选择。</p><dl><dt>Nautilus 版本</dt><dd>{version??'未报告'}</dd><dt>精度</dt><dd>{precision??'未报告'}</dd><dt>账户入口编译</dt><dd>{runner.capabilities.account.compiled?'已编入':'未编入'}</dd><dt>用户账户策略注册入口</dt><dd>{runner.capabilities.account.registration?'已编入':'未编入'}</dd><dt>配置式回测工厂</dt><dd>{runner.capabilities.research.configuredRunFactory?'已编入':'未编入'}</dd></dl><p>这里只显示产物能力查询报告，不验证策略参数、会话在线或账户对账。仍须独立保存账户配置、明确恢复连接及明确启动；旧账户不自动改绑。</p></section>;
}
