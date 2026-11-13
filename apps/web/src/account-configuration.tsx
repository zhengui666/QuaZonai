import AccountStrategy from './account-strategy';
import {useState} from 'react';
import {Button,Input,Checkbox,Select} from 'antd';
import {useResource} from './data-hooks';
import {useCommand} from './commands';
import {ErrorNotice,PendingCommand} from './ui';

export function accountConfigurationInput(text:string):Record<string,unknown>{
 const value:unknown=JSON.parse(text);
 if(!value||typeof value!=='object'||Array.isArray(value))throw new Error('账户配置必须是 JSON 对象');
 return value as Record<string,unknown>;
}
const initialAccountDraft=JSON.stringify({mode:'paper',nativeStrategy:{instrument_id:'',strategy:{kind:'REGISTERED',name:''}}});
/** Field edits preserve unrelated owner values; identities and decimal values are never converted to numbers. */
export function accountConfigurationField(text:string,key:string,value:string):string{
 const config=accountConfigurationInput(text);
 const deployment={...((config.nativeStrategy??{}) as Record<string,unknown>)};
 const strategy={...((deployment.strategy??{}) as Record<string,unknown>)};
 if(['id','clientId','accountId','mode','network','startingBalance','paperFeeRate','accountAddress'].includes(key)){
  if(value==='')delete config[key];else config[key]=value;
 }else if(key==='instruments')config.instruments=value.split('\n').filter(line=>line!=='');
 else if(key==='instrument_id')deployment.instrument_id=value;
 else if(key==='name')strategy.name=value;
 else throw Error('未知账户字段');
 if(key==='name')deployment.strategy=strategy;
 if(key==='name'||key==='instrument_id')config.nativeStrategy=deployment;
 return JSON.stringify(config,null,2);
}
/** Keep textarea editing text verbatim. Only explicit save parses instrument lines. */
export function AccountInstrumentsInput({value,onChange,disabled}:{value:string;onChange:(value:string)=>void;disabled:boolean}){
 return <Input.TextArea aria-label="订阅标的" disabled={disabled} value={value} onChange={event=>onChange(event.target.value)}/>;
}
export function accountConfigurationForSave(text:string,instrumentsDraft?:string){
 return accountConfigurationInput(instrumentsDraft===undefined?text:accountConfigurationField(text,'instruments',instrumentsDraft));
}
export function AccountFields({text,onChange,disabled,instrumentsDraft,onInstrumentsChange}:{text:string;onChange:(text:string)=>void;disabled:boolean;instrumentsDraft?:string;onInstrumentsChange?:(value:string)=>void}){
 let config:Record<string,unknown>;try{config=accountConfigurationInput(text);}catch{return <p>高级配置尚不是有效对象，请先修正；不会保存替代值。</p>;}
 const deployment=config.nativeStrategy as {instrument_id?:string;strategy?:{kind?:string;name?:string}}|undefined;
 const edit=(key:string,value:string)=>onChange(accountConfigurationField(text,key,value));
 const field=(key:string,label:string)=><label style={{display:'block'}} key={key}>{label}<Input aria-label={label} disabled={disabled} value={String(config[key]??'')} onChange={event=>edit(key,event.target.value)}/></label>;
 return <fieldset disabled={disabled}><legend>账户与原生策略</legend>
 {[['id','本地账户名称'],['clientId','客户端 ID'],['accountId','原生账户 ID']].map(([key,label])=>field(key!,label!))}
 <label>账户模式<Select aria-label="账户模式" disabled={disabled} value={config.mode as string} onChange={value=>edit('mode',value)} options={[{value:'paper',label:'模拟盘（paper）'},{value:'live',label:'实盘配置（启动仍受现有准入限制）'}]}/></label>
 <label>网络<Select aria-label="网络" disabled={disabled} allowClear value={config.network as string|undefined} onChange={value=>edit('network',value??'')} options={[{value:'mainnet',label:'主网行情'},{value:'testnet',label:'测试网行情'}]}/></label>
 <label style={{display:'block'}}>订阅标的（每行一个原生 InstrumentId）<AccountInstrumentsInput disabled={disabled} value={instrumentsDraft??(Array.isArray(config.instruments)?config.instruments.join('\n'):'')} onChange={value=>onInstrumentsChange?.(value)}/></label>
 <label style={{display:'block'}}>策略标的（须在订阅列表内）<Input aria-label="策略标的" disabled={disabled} value={deployment?.instrument_id??''} onChange={event=>edit('instrument_id',event.target.value)}/></label>
 {config.mode==='paper'?<>{field('startingBalance','模拟初始余额（USDC 原值）')}{field('paperFeeRate','模拟费率（原始小数，非百分数）')}</>:field('accountAddress','公开账户地址（不是私钥）')}
 {deployment?.strategy?.kind==='REGISTERED'&&<label style={{display:'block'}}>已编入运行器的策略名称<Input aria-label="已编入运行器的策略名称" disabled={disabled} value={deployment.strategy.name??''} onChange={event=>edit('name',event.target.value)}/></label>}
 <p>自定义策略参数没有可用 schema，不能推测字段。请在下方高级配置中按原策略合同填写 config；其他原生策略类型也保留高级入口。选择模式不清除其他字段，提交仍由原保存合同校验。</p>
 </fieldset>;
}
const paperFields = [
 ['prob_fill_on_limit','限价单成交概率（0–1）'],['prob_slippage','滑点概率（0–1）'],
 ['random_seed','随机种子（整数原值）'],['base_latency_ns','基础延迟（ns）'],
 ['insert_latency_ns','下单附加延迟（ns）'],['update_latency_ns','改单附加延迟（ns）'],['cancel_latency_ns','撤单附加延迟（ns）'],
] as const;
export function paperConfigurationField(text:string,key:string,value:string|boolean):string{
 const config=accountConfigurationInput(text);
 if(config.mode!=='paper')throw new Error('模拟参数仅用于 paper 账户');
 if(!paperFields.some(([field])=>field===key)&&key!=='queue_position')throw new Error('未知模拟参数');
 const prior=config.paperExecution;
 if(prior!==undefined&&(!prior||typeof prior!=='object'||Array.isArray(prior)))throw new Error('paperExecution 必须是对象');
 const scenario={...(prior as Record<string,unknown>|undefined)};
 if(value==='')delete scenario[key];
 else if(key==='prob_fill_on_limit'||key==='prob_slippage'){
  const probability=Number(value);
  if(typeof value!=='string'||!value.trim()||!Number.isFinite(probability))throw new Error('概率必须是有效数字');
  scenario[key]=probability;
 }else scenario[key]=value;
 return JSON.stringify({...config,paperExecution:scenario},null,2);
}
/** The native input owns intermediate decimal text until blur. Parsing on every
 * keystroke would turn "0." into "0" and corrupt the next keystroke. */
export function PaperProbabilityInput({label,value,disabled,onCommit}:{label:string;value:unknown;disabled:boolean;onCommit:(text:string)=>boolean|void}){
 return <Input key={String(value??'')} aria-label={label} defaultValue={String(value??'')} disabled={disabled}
  onBlur={event=>{if(onCommit(event.target.value)===false)event.target.value=String(value??'');}} onPressEnter={event=>event.currentTarget.blur()}/>;
}
function PaperParameters({text,onChange,disabled,onError}:{text:string;onChange:(value:string)=>void;disabled:boolean;onError:(error:unknown)=>void}){
 let config:Record<string,unknown>;try{config=accountConfigurationInput(text);}catch{return null;}
 if(config.mode!=='paper')return null;
 const value=config.paperExecution;
 if(value!==undefined&&(!value||typeof value!=='object'||Array.isArray(value)))return <p>请先修正 JSON 中的 paperExecution 对象。</p>;
 const scenario=(value??{}) as Record<string,unknown>;
 const edit=(key:string,value:string|boolean)=>{try{onChange(paperConfigurationField(text,key,value));onError(undefined);return true;}catch(error){onError(new Error(`${error instanceof Error?error.message:'模拟参数无效'}；未写入配置，输入已恢复为原 JSON 值。`));return false;}};
 return <fieldset disabled={disabled}><legend>官方 Sandbox 模拟参数（可选）</legend>
 <p>概率在离开输入框或按回车后写入同一份配置 JSON，其他字段即时写入，不启动账户。启用时请完整填写概率、种子及四项延迟；最终由官方模型校验。纳秒与种子保留整数文本，不换算。未配置时沿用现有官方默认模型，不代表真实成交保证。</p>
 {paperFields.map(([key,label])=><label key={key} style={{display:'block'}}>{label}{key==='prob_fill_on_limit'||key==='prob_slippage'?<PaperProbabilityInput label={label} value={scenario[key]} disabled={disabled} onCommit={value=>edit(key,value)}/>:<Input aria-label={label} value={String(scenario[key]??'')} disabled={disabled} onChange={event=>edit(key,event.target.value)}/>}</label>)}
 <Checkbox checked={scenario.queue_position===true} disabled={disabled} onChange={event=>edit('queue_position',event.target.checked)}>官方队列位置模拟</Checkbox>
 {value!==undefined&&<Button disabled={disabled} onClick={()=>{const next={...config};delete next.paperExecution;onChange(JSON.stringify(next,null,2));}}>移除整组可选模拟参数</Button>}
 </fieldset>;
}
export default function AccountConfiguration({enabled}:{enabled:boolean}){
 const records=useResource<Array<{id:string;unavailable?:string;accountConfiguration:{mode:'paper'|'live';paperExecution?:unknown}}>>('/accounts/configurations');
 const [instrumentsDraft,setInstrumentsDraft]=useState<string>();
 const [text,setText]=useState(initialAccountDraft),[error,setError]=useState<unknown>(),[notice,setNotice]=useState('');
 const command=useCommand<{id:string}>('account-configuration',result=>{setNotice(`账户 ${result.id} 的配置已保存，尚未连接或启动策略。`);void records.refetch();},'/accounts/configurations');
 return <section><h3>保存账户配置，不连接或交易</h3>
 <p>使用上方已选用且包含账户能力的同一 Rust 启动器，不填写第二个 HTTP 地址。当前仅支持 Hyperliquid 的模拟盘或实盘账户。已有历史账户不会被新配置认领、清空或静默改绑。</p>
 <AccountFields text={text} onChange={setText} instrumentsDraft={instrumentsDraft} onInstrumentsChange={setInstrumentsDraft} disabled={!!command.pending||command.busy}/>
 <p>订阅标的保留逐行编辑文本，保存时忽略空行；未保存的标的草稿优先于高级配置。编辑高级配置将清除标的草稿并采用其中的原值。</p>
 <details><summary>高级：原生策略参数及完整配置</summary>
 <p>配置 JSON 需包含 id、clientId、accountId、mode（paper 或 live）、instruments 与 nativeStrategy。模拟盘需 startingBalance、paperFeeRate；实盘需 accountAddress，可选 network。nativeStrategy 为 instrument_id 和 strategy；自定义策略使用 kind: REGISTERED、已编入启动器的 name、原始 config JSON。不要输入私钥、密码或令牌，凭据仍由账户运行环境管理。</p>
 <Input.TextArea aria-label="新账户配置 JSON" autoSize={{minRows:8,maxRows:24}} value={text} disabled={!!command.pending} onChange={event=>{setText(event.target.value);setInstrumentsDraft(undefined);}}/></details>
 <PaperParameters text={text} onChange={setText} disabled={!!command.pending} onError={setError}/>
 {!enabled&&<p>当前尚未选用具有账户能力的构建产物，不能保存新账户。保存环境路径或完成研究构建不等于账户运行器可用。</p>}
 <Button disabled={!enabled||command.busy} loading={command.busy} onClick={()=>{if(command.pending){void command.submit();return;}try{setError(undefined);void command.submit({configuration:accountConfigurationForSave(text,instrumentsDraft)});}catch(failure){setError(failure);}}}>{command.pending?'核实原账户保存请求':'仅保存新账户配置'}</Button>
 <PendingCommand command={command}/><ErrorNotice error={error??command.error??records.error}/>{notice&&<p>{notice}</p>}
 <p>配置保存不会连接。下方每个账户的恢复连接和策略启动均须独立明确操作；恢复连接不代表可交易。</p>
 <Button onClick={()=>void records.refetch()}>只读取已保存账户</Button>
 {records.data?.map(item=><details key={item.id}><summary>{item.id} · {item.accountConfiguration.mode}</summary>{item.accountConfiguration.paperExecution!==undefined&&<pre aria-label="已保存的官方模拟参数原值">{JSON.stringify(item.accountConfiguration.paperExecution,null,2)}</pre>}{item.unavailable?<p>原账户未就绪：{item.unavailable}。保留原身份，不创建替代节点。</p>:<AccountStrategy configured value={{id:item.id,mode:item.accountConfiguration.mode}}/>}</details>)}
 </section>;
}
