import {describe,it,expect} from 'vitest';
import {nativeChartData,nativeNanosecondTime} from './native-chart-data';
describe('original native chart display adapters',()=>{
 it('preserves same-millisecond rows and exact decimal/nanosecond readouts',()=>{
  const rows=[{ts_event_ns:'1776124800000000001',value:0.01},{ts_event_ns:'1776124800000000002',value:null}];
  const result=nativeChartData('$.equity_performance.returns',rows,200)!;
  expect(result.series[0]!.points.map(p=>[p.time,p.row,p.y])).toEqual([['1776124800000000001',200,0.01],['1776124800000000002',201,null]]);
  expect(result.series[0]!.points[0]!.x).toBe(result.series[0]!.points[1]!.x);expect(result.omitted).toBe(1);
  expect(rows[1]!.value).toBeNull();
 });
 it('keeps account and currency separate and does not plot unpriced equity',()=>{
  const rows=[{account_id:'A',ts_event:'1776124800000000001',total_equity:['9007199254740993.00001 USD','2 EUR'],is_stale:true,unpriced_instruments:[]},{account_id:'A',ts_event:'1776124800000000002',total_equity:['10 USD'],unpriced_instruments:['X.SIM']}];
  const result=nativeChartData('$.canonical_result.portfolio_snapshots',rows)!;
  expect(result.series).toHaveLength(2);expect(result.series[0]!.points[0]!.exact).toBe('9007199254740993.00001');
  expect(result.series[0]!.points[0]!.note).toBe('Carried or stale valuation');expect(result.series[0]!.points[1]!.y).toBeNull();expect(result.series[1]!.unit).toBe('EUR');
 });
 it('uses native realized money and explicitly tagged Filled events without reconstructing PnL',()=>{
  const positions=nativeChartData('$.canonical_result.positions',[{position_id:'P',instrument_id:'X.SIM',realized_pnl:'-1.25 USD',ts_closed:null}],20)!;
  expect(positions.title).toBe('Native realized PnL by position record');expect(positions.series[0]!.points[0]).toMatchObject({x:20,y:-1.25,exact:'-1.25'});
  const fills=nativeChartData('$.canonical_result.fills',[{client_order_id:'O',event:{Filled:{ts_event:'1000001',last_px:'0.000000000000000001',currency:'USD',instrument_id:'X.SIM'}}}])!;
  expect(fills.series[0]!.points[0]!.exact).toBe('0.000000000000000001');expect(fills.kind).toBe('scatter');
 });
 it('does not guess unsupported fields, zero-fill or decode hex as decimal',()=>{
  expect(nativeChartData('$.canonical_result.orders',[])).toBeNull();
  expect(nativeChartData('$.canonical_result.positions',[{realized_pnl:'3ff0000000000000'}])!.series).toEqual([]);
  expect(nativeNanosecondTime('bad')).toBeNull();expect(nativeNanosecondTime('9999999999999999999999999')).toBeNull();
 });
});
