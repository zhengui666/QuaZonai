from pathlib import Path
import json
out=Path(__file__).parent
setter='''(memory (export "memory") 1)
(func (export "qz_set_feature_v2") (param $i i32) (param $v f64) (param $mask i32) (param i64 i64)
 (if (i32.lt_u (local.get $i) (i32.const 8)) (then
  (f64.store (i32.mul (local.get $i) (i32.const 8)) (local.get $v))
  (i32.store (i32.add (i32.const 64) (i32.mul (local.get $i) (i32.const 4))) (local.get $mask)))))'''
guard='''(local $i i32) (local $b f64) (local $q f64) (local $dt f64) (local $a f64)
(block $done (loop $check
 (if (i32.ne (i32.load (i32.add (i32.const 64) (i32.mul (local.get $i) (i32.const 4)))) (i32.const 0)) (then unreachable))
 (local.set $i (i32.add (local.get $i) (i32.const 1)))
 (br_if $check (i32.lt_u (local.get $i) (i32.const 8)))))
(local.set $q (f64.load (i32.const 32)))
(local.set $dt (f64.load (i32.const 48)))
(if (i32.or (f64.le (local.get $q) (f64.const 0)) (f64.le (local.get $dt) (f64.const 0))) (then unreachable))
(local.set $b (f64.div (f64.load (i32.const 56)) (local.get $q)))'''
def g(name):return f'(global.get ${name})'
def setg(name,x):return f'(global.set ${name} {x})'
def ema(name,x):return setg(name,f'(f64.add {g(name)} (f64.mul (f64.const 0.2222222222222222) (f64.sub {x} {g(name)})))')
b='(local.get $b)';a='(local.get $a)';dt='(local.get $dt)'
models={}
for name in ['F03','F04','F05','F06']:
 globals=['init']+({'F03':['e'],'F04':['e'],'F05':['e','mt','st','ma','sa'],'F06':['u','v']}[name]);decl='\n'.join(f'(global ${x} (mut {"i32" if x=="init" else "f64"}) ({"i32" if x=="init" else "f64"}.const 0))' for x in globals)
 pre='';seed=[];update=[]
 if name=='F03':seed=[setg('e',b)];update=[ema('e',b)];score=g('e')
 if name=='F04':
  pre=f'(local.set $a (f64.abs {b}))';seed=[setg('e',a)];update=[ema('e',a)]
  score=f'(f64.mul (f64.neg (f64.sub (f64.div (f64.load (i32.const 0)) (f64.load (i32.const 8))) (f64.const 1))) (f64.max (f64.const 0) (f64.sub {g("e")} {a})))'
 if name=='F05':
  pre='''(if (f64.le (f64.load (i32.const 40)) (f64.const 0)) (then unreachable))
(local.set $a (f64.div (local.get $q) (f64.load (i32.const 40))))'''
  seed=[setg('e',b),setg('mt',dt),setg('st',f'(f64.mul {dt} {dt})'),setg('ma',a),setg('sa',f'(f64.mul {a} {a})')]
  update=[ema('e',b),ema('mt',dt),ema('st',f'(f64.mul {dt} {dt})'),ema('ma',a),ema('sa',f'(f64.mul {a} {a})')]
  def cv(m,s):return f'(f64.div (f64.sqrt (f64.max (f64.const 0) (f64.sub {g(s)} (f64.mul {g(m)} {g(m)})))) {g(m)})'
  score=f'(f64.div {g("e")} (f64.mul (f64.add (f64.const 1) {cv("mt","st")}) (f64.add (f64.const 1) {cv("ma","sa")})))'
 if name=='F06':
  pre='(local.set $a (f64.div (f64.const 30000000000) (f64.add (f64.const 30000000000) (local.get $dt))))'
  seed=[setg('u','(f64.load (i32.const 56))'),setg('v','(local.get $q)')]
  update=[setg('u',f'(f64.add (f64.mul {a} {g("u")}) (f64.load (i32.const 56)))'),setg('v',f'(f64.add (f64.mul {a} {g("v")}) (local.get $q))')];score=f'(f64.div {g("u")} {g("v")})'
 state='(if (i32.eqz (global.get $init)) (then '+ ' '.join(seed)+' (global.set $init (i32.const 1))) (else '+' '.join(update)+'))'
 wat=f'(module\n{setter}\n{decl}\n(func (export "qz_predict_v2") (param i64 i64 i32 i32 i32) (result f64)\n{guard}\n{pre}\n{state}\n{score}))\n';(out/(name+'.wat')).write_text(wat);models[name]=wat
(out/'models.json').write_text(json.dumps(models,indent=2)+'\n')
