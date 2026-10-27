(module (type $t (func (result i32))) (elem declare func $g) (func $g (result i32) (i32.const 1)) (func (export "f") (result i32) (call_ref $t (ref.func $g))))
