export const en: Record<string,string> = Object.fromEntries([
 'Official daily return','Official daily returns','Native account equity','Native realized PnL by position record','Native fill prices','Unknown instrument',
 'Chart of the loaded evidence page. Exact values are available in the table.','Original timestamp (ns)','Not available','Original row','Currency or unit','Reset range',
 'Missing, invalid or unpriced values not drawn: {{count}}','Unpriced instruments; not plotted','Carried or stale valuation',
 'Rows {{start}}–{{end}} of {{total}}. This is the loaded page, not the full history.',
 'The original table is empty. No trading or risk conclusion follows from an empty table.',
 'No supported numeric series on this page. Original evidence remains in the table.','Original page values','Previous page','Next page',
 'Charts and original evidence','Task {{taskId}}. Charts use original native values; they do not create new performance metrics.',
 'No retained tables are available for this result.','Original result table','Previous tables','More tables',
].map(value=>[value,value]));
export const zhCN: Record<string,string> = {
 'Official daily return':'官方日收益率','Official daily returns':'官方日收益率',
 'Native account equity':'原生账户权益','Native realized PnL by position record':'各原生持仓记录的已实现盈亏','Native fill prices':'原生成交价格','Unknown instrument':'未知标的',
 'Chart of the loaded evidence page. Exact values are available in the table.':'仅绘制已加载证据页，精确原值见下表。',
 'Original timestamp (ns)':'原始时间戳（纳秒）','Not available':'不可用','Original row':'原始行号','Currency or unit':'币种或单位','Reset range':'重置范围',
 'Missing, invalid or unpriced values not drawn: {{count}}':'未绘制的缺失、无效或未估值数值：{{count}}',
 'Unpriced instruments; not plotted':'存在未估值标的，未绘制','Carried or stale valuation':'沿用或陈旧估值',
 'Rows {{start}}–{{end}} of {{total}}. This is the loaded page, not the full history.':'第 {{start}}–{{end}} 行，共 {{total}} 行。仅为已加载页，并非完整历史。',
 'The original table is empty. No trading or risk conclusion follows from an empty table.':'原表为空，不能由此推导交易或风险结论。',
 'No supported numeric series on this page. Original evidence remains in the table.':'本页没有适合当前图形的数值序列，原始证据仍保留在表中。',
 'Original page values':'本页原始数值','Previous page':'上一页','Next page':'下一页','Charts and original evidence':'图表与原始证据',
 'Task {{taskId}}. Charts use original native values; they do not create new performance metrics.':'任务 {{taskId}}。图表使用原生原值，不生成新的表现指标。',
 'No retained tables are available for this result.':'此结果没有可读取的已保留表。','Original result table':'原始结果表','Previous tables':'上一组表','More tables':'更多表',
};
Object.assign(en,Object.fromEntries(['Field absent','Original native runs','Choose an original run. Failed and unfinished runs remain visible; a parameter case is not a selected winner.','Original native task','Choose an original run','This result request','Historical request','This original task did not complete successfully. No result chart is inferred.','Previous runs','More runs'].map(value=>[value,value])));
Object.assign(zhCN,{'Field absent':'字段不存在','Original native runs':'原生运行原记录','Choose an original run. Failed and unfinished runs remain visible; a parameter case is not a selected winner.':'选择原始运行。失败和未完成记录仍保留；某组参数结果并不代表择优。','Original native task':'原生任务','Choose an original run':'选择原始运行','This result request':'本次结果请求','Historical request':'历史请求','This original task did not complete successfully. No result chart is inferred.':'原任务未成功完成，不据此推断结果图表。','Previous runs':'上一组运行','More runs':'更多运行'});
