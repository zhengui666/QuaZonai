from pathlib import Path
import json

out = Path(__file__).parent
# Reuse the checked L01 interval window without executing or modifying its builder.
wat = (out.parent / 'planned-liquidity-probe/L01.wat').read_text()
wat = wat.replace('(global $count', '(global $z (mut i32) (i32.const 0))\n(global $count', 1)
wat = wat.replace('(local $dt f64)', '(local $dt f64) (local $g f64)', 1)
wat = wat.replace('(call $positive_finite (f64.load (i32.const 0)))', '(call $positive_finite (f64.load (i32.const 24)))', 1)
wat = wat.replace('(call $positive_finite (local.get $dt))', '''(call $positive_finite (local.get $dt))
 (local.set $g (f64.div (f64.sub (f64.load (i32.const 16)) (f64.load (i32.const 24))) (f64.load (i32.const 24))))
 (if (i32.eqz (f64.lt (f64.abs (local.get $g)) (f64.const inf))) (then unreachable))''', 1)
wat = wat.replace('(then (return (f64.const 0)))', '(then (global.set $z (i32.const 0)) (return (f64.const 0)))')
old = '(f64.neg (f64.div (f64.sub (f64.load (i32.const 0)) (f64.load (i32.const 16))) (f64.load (i32.const 16)))))'
new = '''(if (f64.ge (local.get $g) (f64.const 0.0018)) (then (global.set $z (i32.const 1)))
  (else (if (f64.le (local.get $g) (f64.const -0.0018)) (then (global.set $z (i32.const -1)))
   (else (if (f64.le (f64.mul (local.get $g) (f64.convert_i32_s (global.get $z))) (f64.const 0))
    (then (global.set $z (i32.const 0))))))))
 (f64.convert_i32_s (global.get $z)))'''
assert old in wat
wat = wat.replace(old, new, 1)
(out / 'T01.wat').write_text(wat)
(out / 'models.json').write_text(json.dumps({'T01': wat}, indent=2) + '\n')
