import {useEffect,useRef,useState,useMemo} from 'react';
import {Button,Select,theme} from 'antd';
import * as echarts from 'echarts/core';
import {LineChart,ScatterChart,BarChart} from 'echarts/charts';
import {GridComponent,TooltipComponent,DataZoomComponent,AriaComponent,LegendComponent} from 'echarts/components';
import {SVGRenderer} from 'echarts/renderers';
import {useTranslation} from 'react-i18next';
import type {NativeChartData} from './native-chart-data';
echarts.use([LineChart,ScatterChart,BarChart,GridComponent,TooltipComponent,DataZoomComponent,AriaComponent,LegendComponent,SVGRenderer]);
/** Shared display only: original data and statistics are owned by Nautilus. */
export default function ResearchChart({data}:{data:NativeChartData}){
 const {t,i18n}=useTranslation('charts');const element=useRef<HTMLDivElement>(null),chart=useRef<echarts.EChartsType|undefined>(undefined);const {token}=theme.useToken();
 const units=[...new Set(data.series.map(s=>s.unit))];const [selectedUnit,setUnit]=useState('');const unit=units.includes(selectedUnit)?selectedUnit:units[0];
 const visible=useMemo(()=>data.series.filter(s=>s.unit===unit).map(s=>({...s,points:data.xKind==='time'?[...s.points].sort((a,b)=>{const x=BigInt(a.time),y=BigInt(b.time);return x<y?-1:x>y?1:0;}):s.points})),[data,unit]);
 const title=t(data.title);const description=t('Chart of the loaded evidence page. Exact values are available in the table.');
 useEffect(()=>{if(!element.current)return;const instance=echarts.init(element.current,undefined,{renderer:'svg',locale:i18n.language.startsWith('zh')?'ZH':'EN'});chart.current=instance;const observer=new ResizeObserver(()=>instance.resize());observer.observe(element.current);return()=>{observer.disconnect();instance.dispose();chart.current=undefined;};},[i18n.language]);
 useEffect(()=>{
  chart.current?.setOption({animation:false,useUTC:true,aria:{enabled:true,label:{description}},textStyle:{color:token.colorText},
   legend:{type:'scroll',top:0,textStyle:{color:token.colorText}},grid:{left:70,right:24,top:55,bottom:70},
   tooltip:{trigger:'item',confine:true,renderMode:'richText',formatter:(params:unknown)=>{const p=params as {seriesIndex:number;dataIndex:number};const s=visible[p.seriesIndex],v=s?.points[p.dataIndex];return v&&s?`${t(s.name)}\n${v.time?`${t('Original timestamp (ns)')}: ${v.time}\n`:''}${v.exact||t('Not available')} ${s.unit}${v.note?`\n${t(v.note)}`:''}\n${t('Original row')}: ${v.row}`:'';}},
   xAxis:{type:data.xKind==='time'?'time':'value',name:data.xKind==='time'?'UTC':t('Original row'),axisLabel:{hideOverlap:true}},
   yAxis:{type:'value',scale:true,axisLabel:{formatter:(n:number)=>new Intl.NumberFormat(i18n.language,{maximumSignificantDigits:6}).format(n)}},
   dataZoom:[{type:'slider',filterMode:'none',bottom:8},{type:'inside',filterMode:'none',zoomOnMouseWheel:'ctrl'}],
   series:visible.map((s,index)=>({id:String(index),name:`${t(s.name)} · ${s.unit}`,type:data.kind,data:s.points.map(p=>[p.x,p.y]),connectNulls:false,smooth:false,showSymbol:true,symbolSize:4})),
  },{notMerge:true});
 },[data,visible,description,i18n.language,t,token.colorText]);
 return <section style={{minWidth:0}}><h4>{title}</h4><p>{description}</p>{units.length>1&&<Select aria-label={t('Currency or unit')} value={unit} options={units.map(value=>({value,label:value}))} onChange={setUnit}/>}<div ref={element} role="img" aria-label={title} style={{height:320,width:'100%',minWidth:0}}/><Button size="small" onClick={()=>chart.current?.dispatchAction({type:'dataZoom',start:0,end:100})}>{t('Reset range')}</Button>{data.omitted>0&&<p role="note">{t('Missing, invalid or unpriced values not drawn: {{count}}',{count:data.omitted})}</p>}</section>;
}
