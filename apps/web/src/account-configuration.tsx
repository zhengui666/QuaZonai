import AccountStrategy from './account-strategy';
import {useState} from 'react';
import {Button,Input} from 'antd';
import {useResource} from './data-hooks';
import {useCommand} from './commands';
import {ErrorNotice,PendingCommand} from './ui';

export function accountConfigurationInput(text:string):Record<string,unknown>{
 const value:unknown=JSON.parse(text);
 if(!value||typeof value!=='object'||Array.isArray(value))throw new Error('账户配置必须是 JSON 对象');
 return value as Record<string,unknown>;
}
export default function AccountConfiguration({enabled}:{enabled:boolean}){
 const records=useResource<Array<{id:string;unavailable?:string;accountConfiguration:{mode:'paper'|'live'}}>>('/accounts/configurations');
 const [text,setText]=useState(''),[error,setError]=useState<unknown>(),[notice,setNotice]=useState('');
 const command=useCommand<{id:string}>('account-configuration',result=>{setNotice(`账户 ${result.id} 的配置已保存，尚未连接或启动策略。`);void records.refetch();},'/accounts/configurations');
 return <section><h3>保存账户配置，不连接或交易</h3>
 <p>使用上方已选用且包含账户能力的同一 Rust 启动器，不填写第二个 HTTP 地址。当前仅支持 Hyperliquid 的模拟盘或实盘账户。已有历史账户不会被新配置认领、清空或静默改绑。</p>
 <p>配置 JSON 需包含 id、clientId、accountId、mode（paper 或 live）、instruments 与 nativeStrategy。模拟盘需 startingBalance、paperFeeRate；实盘需 accountAddress，可选 network。nativeStrategy 为 instrument_id 和 strategy；自定义策略使用 kind: REGISTERED、已编入启动器的 name、原始 config JSON。不要输入私钥、密码或令牌，凭据仍由账户运行环境管理。</p>
 <Input.TextArea aria-label="新账户配置 JSON" autoSize={{minRows:8,maxRows:24}} value={text} disabled={!!command.pending} onChange={event=>setText(event.target.value)}/>
 {!enabled&&<p>当前尚未选用具有账户能力的构建产物，不能保存新账户。保存环境路径或完成研究构建不等于账户运行器可用。</p>}
 <Button disabled={!enabled||command.busy} loading={command.busy} onClick={()=>{if(command.pending){void command.submit();return;}try{setError(undefined);void command.submit({configuration:accountConfigurationInput(text)});}catch(failure){setError(failure);}}}>{command.pending?'核实原账户保存请求':'仅保存新账户配置'}</Button>
 <PendingCommand command={command}/><ErrorNotice error={error??command.error??records.error}/>{notice&&<p>{notice}</p>}
 <p>配置保存不会连接。下方每个账户的恢复连接和策略启动均须独立明确操作；恢复连接不代表可交易。</p>
 <Button onClick={()=>void records.refetch()}>只读取已保存账户</Button>
 {records.data?.map(item=><details key={item.id}><summary>{item.id} · {item.accountConfiguration.mode}</summary>{item.unavailable?<p>原账户未就绪：{item.unavailable}。保留原身份，不创建替代节点。</p>:<AccountStrategy configured value={{id:item.id,mode:item.accountConfiguration.mode}}/>}</details>)}
 </section>;
}
