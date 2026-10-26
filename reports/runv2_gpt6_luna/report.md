# wasm2llvm verification report

- build: **ok**
- **correctness score (primary): 0.8101**
- validation score (secondary; invalid-rejection rate x valid-module acceptance rate): 0.8620
- SIMD lowering quality (secondary): 221/234 = 0.944
- performance (secondary): geomean speedup vs wasmtime = 1.116

| suite | weight | exec score | scripts fully passing | assertions | validation |
|---|---|---|---|---|---|
| spec | 0.3 | 0.9044 | 102/120 | 43822/45888 | 2853/2865 |
| spec3 | 0.1 | 0.9544 | 20/23 | 5860/5874 | 1491/1494 |
| payloads | 0.2 | 0.8667 | 39/45 | 296/314 | - |
| real | 0.2 | 1.0000 | 11/11 | 74/74 | - |
| smith | 0.2 | 0.3503 | 165/471 | 584/8181 | - |

The `real` suite is scored all-or-nothing per script; its assertion totals are diagnostic.

## Failures (first per script)

- `spec:binary/binary.json`: line 146 assert_malformed [native-O0]: module was accepted
- `spec:br_if/br_if.json`: line 593 assert_invalid [native-O0]: module was accepted
- `spec:conversions/conversions.json`: line 123 assert_return [native-O0]: got 'trap 2'
- `spec:float_exprs/float_exprs.json`: line 75 assert_return [native-O2]: result 0: got 0000a07f000000000000000000000000, expected {'type': 'f32', 'value': 'nan:arithmetic'}
- `spec:return/return.json`: line 360 assert_invalid [native-O0]: module was accepted
- `spec:simd_const/simd_const.json`: line 1064 assert_return [native-O0]: module at line 1031 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `spec:simd_f32x4_pmin_pmax/simd_f32x4_pmin_pmax.json`: line 12 assert_return [native-O0]: result 0: got 00000080000000800000008000000080, expected {'type': 'v128', 'lane_type': 'f32', 'value': ['0', '0', '0', '0']}
- `spec:simd_f64x2_pmin_pmax/simd_f64x2_pmin_pmax.json`: line 12 assert_return [native-O0]: result 0: got 00000000000000800000000000000080, expected {'type': 'v128', 'lane_type': 'f64', 'value': ['0', '0']}
- `spec:simd_f64x2_rounding/simd_f64x2_rounding.json`: line 11 assert_return [native-O0]: module at line 4 unavailable: translation failed: translator exit status 1: wasm2llvm: function 0: operand stack underflow (at instruction 1: SIMD opcode 0x74)

- `spec:simd_i16x8_arith2/simd_i16x8_arith2.json`: line 33 assert_return [native-O0]: module at line 3 unavailable: clang failed: /tmp/w2l-verify-o18jztez/native/cache/31f3c94875a4ce0a6807a099.ll:310:15: error: invalid cast opcode for cast from '<8 x i16>' to '<4 x i32>'
  310 |   %t10
- `spec:simd_i16x8_extadd_pairwise_i8x16/simd_i16x8_extadd_pairwise_i8x16.json`: line 11 assert_return [native-O0]: module at line 4 unavailable: clang failed: /tmp/w2l-verify-o18jztez/native/cache/1a73b43ba37263e6080f4059.ll:155:24: error: '%t4' defined with type '<8 x i8>' but expected '<16 x i8>'
  155 |   %t6 =
- `spec:simd_i16x8_q15mulr_sat_s/simd_i16x8_q15mulr_sat_s.json`: line 10 assert_return [native-O0]: module at line 4 unavailable: clang failed: /tmp/w2l-verify-o18jztez/native/cache/4c840b33d12cf9fd2ed36575.ll:165:30: error: constant expression type mismatch: got type '<4 x i32>' but expected '<8 x 
- `spec:simd_i32x4_extadd_pairwise_i16x8/simd_i32x4_extadd_pairwise_i16x8.json`: line 11 assert_return [native-O0]: module at line 4 unavailable: clang failed: /tmp/w2l-verify-o18jztez/native/cache/66afe4c13d08dc34f795a5d6.ll:156:24: error: '%t5' defined with type '<4 x i16>' but expected '<8 x i16>'
  156 |   %t7 
- `spec:simd_i8x16_arith2/simd_i8x16_arith2.json`: line 35 assert_return [native-O0]: module at line 3 unavailable: clang failed: /tmp/w2l-verify-o18jztez/native/cache/da62ec4963034470d562ef48.ll:296:14: error: invalid cast opcode for cast from '<16 x i8>' to '<8 x i16>'
  296 |   %t8 
- `spec:simd_lane/simd_lane.json`: line 858 assert_return [native-O0]: module at line 830 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `spec:simd_splat/simd_splat.json`: line 371 assert_return [native-O0]: module at line 347 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `spec:simd_store16_lane/simd_store16_lane.json`: line 193 assert_return [native-O0]: module at line 4 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `spec:simd_store32_lane/simd_store32_lane.json`: line 125 assert_return [native-O0]: module at line 4 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `spec:simd_store64_lane/simd_store64_lane.json`: line 79 assert_return [native-O0]: module at line 4 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `spec:simd_store8_lane/simd_store8_lane.json`: line 281 assert_return [native-O0]: module at line 4 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `spec:unreached-valid/unreached-valid.json`: line 63 assert_trap [native-O0]: module at line 49 unavailable: translation failed: translator exit status 1: wasm2llvm: function 0: br_table target label types do not match (at instruction 2: BrTable([0, 1], 1)) (at instruction 0: b
- `spec3:binary/binary.json`: line 633 assert_malformed [native-O0]: module was accepted
- `spec3:br_if/br_if.json`: line 593 assert_invalid [native-O0]: module was accepted
- `spec3:simd__simd_const/simd__simd_const.json`: line 1068 assert_return [native-O0]: module at line 1035 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `spec3:simd__simd_lane/simd__simd_lane.json`: line 858 assert_return [native-O0]: module at line 830 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `spec3:unreached-valid/unreached-valid.json`: line 77 assert_trap [native-O0]: module at line 63 unavailable: translation failed: translator exit status 1: wasm2llvm: function 0: br_table target label types do not match (at instruction 2: BrTable([0, 1], 1)) (at instruction 0: b
- `payloads:simd-O0/simd-O0.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: function 43: type mismatch, expected v128 (at instruction 15: SIMD opcode 0x75)

- `payloads:simd-O3/simd-O3.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: function 13: type mismatch, expected v128 (at instruction 51: SIMD opcode 0x75)

- `payloads:simd-Os/simd-Os.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: function 15: type mismatch, expected v128 (at instruction 51: SIMD opcode 0x75)

- `payloads:simd_image-O0/simd_image-O0.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: clang failed: /tmp/w2l-verify-o18jztez/native/cache/f4417ad29db0225bc0820ffe.ll:6134:15: error: invalid cast opcode for cast from '<16 x i8>' to '<8 x i16>'
 6134 |   %t9
- `payloads:simd_image-O3/simd_image-O3.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: clang failed: /tmp/w2l-verify-o18jztez/native/cache/2e3b2b95096de9f1742cc65e.ll:4325:34: error: constant expression type mismatch: got type '<4 x i32>' but expected '<8 x
- `payloads:simd_image-Os/simd_image-Os.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: clang failed: /tmp/w2l-verify-o18jztez/native/cache/83b672642a8555448c63b609.ll:2363:34: error: constant expression type mismatch: got type '<4 x i32>' but expected '<8 x
- `smith:smith1000000/smith1000000.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith1000001/smith1000001.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith1000002/smith1000002.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith1000003/smith1000003.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith1000004/smith1000004.json`: line 2 assert_memhash [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith1000006/smith1000006.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith1000008/smith1000008.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith1000009/smith1000009.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith1000010/smith1000010.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith1000011/smith1000011.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith1000012/smith1000012.json`: line 2 assert_memhash [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith1000013/smith1000013.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith1000014/smith1000014.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith1000015/smith1000015.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith1000017/smith1000017.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith1000019/smith1000019.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith1000020/smith1000020.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith1000021/smith1000021.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith1000022/smith1000022.json`: line 2 assert_memhash [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith1000024/smith1000024.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith1000028/smith1000028.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith1000032/smith1000032.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith1000034/smith1000034.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith1000035/smith1000035.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith1000036/smith1000036.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith1000038/smith1000038.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith1000040/smith1000040.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith1000041/smith1000041.json`: line 2 assert_memhash [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith1000043/smith1000043.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith1000045/smith1000045.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith1000046/smith1000046.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith1000050/smith1000050.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith1000052/smith1000052.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith1000053/smith1000053.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith1000055/smith1000055.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith1000057/smith1000057.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith1000058/smith1000058.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith1000059/smith1000059.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000005/smith2000005.json`: line 2 assert_memhash [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000007/smith2000007.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000008/smith2000008.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000009/smith2000009.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000013/smith2000013.json`: line 2 assert_memhash [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000017/smith2000017.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000018/smith2000018.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000019/smith2000019.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000026/smith2000026.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000029/smith2000029.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000030/smith2000030.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000031/smith2000031.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000032/smith2000032.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000033/smith2000033.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000034/smith2000034.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000037/smith2000037.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000040/smith2000040.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000041/smith2000041.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000043/smith2000043.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000046/smith2000046.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000048/smith2000048.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000049/smith2000049.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000051/smith2000051.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000055/smith2000055.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000056/smith2000056.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000058/smith2000058.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000059/smith2000059.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000060/smith2000060.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000062/smith2000062.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000064/smith2000064.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000066/smith2000066.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000069/smith2000069.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000074/smith2000074.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000075/smith2000075.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000079/smith2000079.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000081/smith2000081.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000082/smith2000082.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000084/smith2000084.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000085/smith2000085.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000087/smith2000087.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000089/smith2000089.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000090/smith2000090.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000092/smith2000092.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000094/smith2000094.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000096/smith2000096.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000097/smith2000097.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000098/smith2000098.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000101/smith2000101.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000102/smith2000102.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000103/smith2000103.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000110/smith2000110.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000113/smith2000113.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000114/smith2000114.json`: line 2 assert_memhash [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000115/smith2000115.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000116/smith2000116.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000118/smith2000118.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000120/smith2000120.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000121/smith2000121.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000123/smith2000123.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000124/smith2000124.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000127/smith2000127.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000129/smith2000129.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000131/smith2000131.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000132/smith2000132.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000133/smith2000133.json`: line 2 assert_memhash [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000134/smith2000134.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000137/smith2000137.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000138/smith2000138.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000139/smith2000139.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000140/smith2000140.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000141/smith2000141.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000143/smith2000143.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000144/smith2000144.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000145/smith2000145.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000146/smith2000146.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000147/smith2000147.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000149/smith2000149.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000150/smith2000150.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000153/smith2000153.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000154/smith2000154.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000155/smith2000155.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000156/smith2000156.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000157/smith2000157.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000159/smith2000159.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000162/smith2000162.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000163/smith2000163.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000165/smith2000165.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000166/smith2000166.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000168/smith2000168.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000170/smith2000170.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000172/smith2000172.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000173/smith2000173.json`: line 2 assert_memhash [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000174/smith2000174.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000176/smith2000176.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000178/smith2000178.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000179/smith2000179.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000180/smith2000180.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000183/smith2000183.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000184/smith2000184.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000187/smith2000187.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000188/smith2000188.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000189/smith2000189.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000190/smith2000190.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000197/smith2000197.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000200/smith2000200.json`: line 2 assert_memhash [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000201/smith2000201.json`: line 2 assert_memhash [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000202/smith2000202.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000203/smith2000203.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000204/smith2000204.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000205/smith2000205.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000206/smith2000206.json`: line 2 assert_memhash [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000207/smith2000207.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000211/smith2000211.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000212/smith2000212.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000213/smith2000213.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000214/smith2000214.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000215/smith2000215.json`: line 2 assert_memhash [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000216/smith2000216.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000217/smith2000217.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000223/smith2000223.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000224/smith2000224.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000226/smith2000226.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000227/smith2000227.json`: line 2 assert_memhash [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000228/smith2000228.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000229/smith2000229.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000230/smith2000230.json`: line 2 assert_memhash [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000231/smith2000231.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000233/smith2000233.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000235/smith2000235.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000239/smith2000239.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000240/smith2000240.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000242/smith2000242.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000244/smith2000244.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000245/smith2000245.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000246/smith2000246.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000247/smith2000247.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000252/smith2000252.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000254/smith2000254.json`: line 2 assert_memhash [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000255/smith2000255.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000257/smith2000257.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000261/smith2000261.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000262/smith2000262.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000263/smith2000263.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000265/smith2000265.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000274/smith2000274.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000276/smith2000276.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000277/smith2000277.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000278/smith2000278.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000283/smith2000283.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000287/smith2000287.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000288/smith2000288.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000292/smith2000292.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000293/smith2000293.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000295/smith2000295.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000296/smith2000296.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000297/smith2000297.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000298/smith2000298.json`: line 2 assert_memhash [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000299/smith2000299.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000300/smith2000300.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000305/smith2000305.json`: line 2 assert_trap [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000310/smith2000310.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd

- `smith:smith2000312/smith2000312.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: translation failed: translator exit status 1: wasm2llvm: unsupported constant expression opcode 0xfd


## SIMD ops not passing the lowering check

- i8x16.avgr_u: opt-failed (lane ops None, allowed 2)
- i16x8.avgr_u: opt-failed (lane ops None, allowed 2)
- i16x8.q15mulr_sat_s: opt-failed (lane ops None, allowed 2)
- f32x4.pmin: incorrect (lane ops 0, allowed 2)
- f32x4.pmax: incorrect (lane ops 0, allowed 2)
- f64x2.pmin: incorrect (lane ops 0, allowed 2)
- f64x2.pmax: incorrect (lane ops 0, allowed 2)
- i16x8.extadd_pairwise_i8x16_s: opt-failed (lane ops None, allowed 2)
- i16x8.extadd_pairwise_i8x16_u: opt-failed (lane ops None, allowed 2)
- i32x4.extadd_pairwise_i16x8_s: opt-failed (lane ops None, allowed 2)
- i32x4.extadd_pairwise_i16x8_u: opt-failed (lane ops None, allowed 2)
- f64x2.ceil: translate-failed (lane ops None, allowed 2)
- f64x2.floor: translate-failed (lane ops None, allowed 2)

## Performance

| bench | native -O2 (s) | wasmtime (s) | speedup |
|---|---|---|---|
| compress-O3 | 1.002 | 0.733 | 0.731  |
| crypto-O3 | 0.680 | 0.700 | 1.030  |
| nbody-O3 | 0.791 | 0.755 | 0.955  |
| numerics-O3 | 1.906 | 1.626 | 0.853  |
| raytrace-O3 | 1.615 | 1.436 | 0.889  |
| simd_image-O3 | - | 0.848 | - prepare failed: clang failed: /tmp/w2l-verify-o18jztez/native/cache/2e3b2b95096de9f1742cc65e.ll:4325:34: error: constant expression type mismatch: got type '<4 x i32>' but expected '<8 x i32>'
 4325 |   %t3099 = add <8 x i32> %t3098, <i32 16384, i32 16384, i32 16384, i32 16384>
      |                               |
| text-O3 | 0.096 | 0.306 | 3.175  |
| vm-O3 | 1.781 | 2.211 | 1.241  |
