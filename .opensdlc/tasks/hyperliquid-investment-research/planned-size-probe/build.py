from pathlib import Path
import json

out = Path(__file__).parent
# Reuse checked ABI setter and finite guards without running previous builders.
header = (out.parent / 'planned-absorption-probe/A01.wat').read_text().split('(global $count')[0]
wat = header + '''(global $count (mut i32) (i32.const 0))
(global $next (mut i32) (i32.const 0))
(func (export "qz_predict_v2") (param i64 i64 i32 i32 i32) (result f64)
 (local $i i32) (local $at i32) (local $m f64) (local $b f64)
 (local $mean f64) (local $recent_dt f64) (local $score f64)
 (loop $masks
  (if (i32.ne (i32.load (i32.add (i32.const 64) (i32.mul (local.get $i) (i32.const 4)))) (i32.const 0)) (then unreachable))
  (local.set $i (i32.add (local.get $i) (i32.const 1)))
  (br_if $masks (i32.lt_u (local.get $i) (i32.const 8))))
 (call $positive_finite (f64.load (i32.const 32)))
 (call $positive_finite (f64.load (i32.const 40)))
 (call $positive_finite (f64.load (i32.const 48)))
 (call $finite (f64.load (i32.const 56)))
 (local.set $m (f64.div (f64.load (i32.const 32)) (f64.load (i32.const 40))))
 (local.set $b (f64.div (f64.load (i32.const 56)) (f64.load (i32.const 32))))
 (call $positive_finite (local.get $m))
 (call $finite (local.get $b))
 (if (i32.eq (global.get $count) (i32.const 32)) (then
  (local.set $i (i32.const 0))
  (local.set $recent_dt (f64.load (i32.const 48)))
  (loop $history
   (local.set $at (i32.add (i32.const 128) (i32.mul (local.get $i) (i32.const 16))))
   (local.set $mean (f64.add (local.get $mean) (f64.div (f64.load (local.get $at)) (f64.const 32))))
   ;; Mean uses all 32 previous m; activity uses current dt plus previous 31 dt.
   (if (i32.ne (local.get $i) (global.get $next)) (then
    (local.set $recent_dt (f64.add (local.get $recent_dt) (f64.load offset=8 (local.get $at))))))
   (local.set $i (i32.add (local.get $i) (i32.const 1)))
   (br_if $history (i32.lt_u (local.get $i) (i32.const 32))))
  (call $positive_finite (local.get $mean))
  (if (f64.le (local.get $recent_dt) (f64.const 60000000000)) (then
   (local.set $score (f64.mul (local.get $b)
    (f64.max (f64.const 0) (f64.sub (f64.div (local.get $m) (local.get $mean)) (f64.const 2)))))
   (call $finite (local.get $score))))))
 ;; Commit the current observation only after scoring against past state.
 (local.set $at (i32.add (i32.const 128) (i32.mul (global.get $next) (i32.const 16))))
 (f64.store (local.get $at) (local.get $m))
 (f64.store offset=8 (local.get $at) (f64.load (i32.const 48)))
 (global.set $next (i32.and (i32.add (global.get $next) (i32.const 1)) (i32.const 31)))
 (if (i32.lt_u (global.get $count) (i32.const 32)) (then
  (global.set $count (i32.add (global.get $count) (i32.const 1)))))
 (local.get $score))
)
'''
(out / 'S01.wat').write_text(wat)
(out / 'models.json').write_text(json.dumps({'S01': wat}, indent=2) + '\n')
