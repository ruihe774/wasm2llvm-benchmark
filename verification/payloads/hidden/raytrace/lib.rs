//! Small f32 ray tracer (spheres, planes, shadows, reflections). Exact float
//! reproducibility matters: no FMA contraction is allowed in a correct translation.
#![no_std]
#![feature(wasm_numeric_instr)]

#[path = "../../common/common.rs"]
mod common;
use common::*;
use core::arch::wasm32::f32_sqrt;

#[derive(Clone, Copy)]
struct V(f32, f32, f32);
impl V {
    fn add(self, o: V) -> V { V(self.0 + o.0, self.1 + o.1, self.2 + o.2) }
    fn sub(self, o: V) -> V { V(self.0 - o.0, self.1 - o.1, self.2 - o.2) }
    fn mul(self, k: f32) -> V { V(self.0 * k, self.1 * k, self.2 * k) }
    fn dot(self, o: V) -> f32 { self.0 * o.0 + self.1 * o.1 + self.2 * o.2 }
    fn norm(self) -> V { let l = f32_sqrt(self.dot(self)); self.mul(1.0 / l) }
    fn hadamard(self, o: V) -> V { V(self.0 * o.0, self.1 * o.1, self.2 * o.2) }
}

struct Sphere { c: V, r: f32, col: V, refl: f32 }

const SPHERES: [Sphere; 5] = [
    Sphere { c: V(0.0, -1000.0, 0.0), r: 999.0, col: V(0.5, 0.5, 0.5), refl: 0.1 },
    Sphere { c: V(0.0, 0.0, 5.0), r: 1.0, col: V(1.0, 0.2, 0.2), refl: 0.4 },
    Sphere { c: V(2.0, 0.3, 6.0), r: 1.3, col: V(0.2, 1.0, 0.2), refl: 0.2 },
    Sphere { c: V(-2.2, 0.1, 5.5), r: 1.1, col: V(0.2, 0.3, 1.0), refl: 0.6 },
    Sphere { c: V(0.4, -0.6, 3.5), r: 0.4, col: V(1.0, 1.0, 0.1), refl: 0.0 },
];

fn hit(o: V, d: V) -> Option<(f32, usize)> {
    let mut best: Option<(f32, usize)> = None;
    for (i, s) in SPHERES.iter().enumerate() {
        let oc = o.sub(s.c);
        let b = oc.dot(d);
        let c = oc.dot(oc) - s.r * s.r;
        let disc = b * b - c;
        if disc > 0.0 {
            let sq = f32_sqrt(disc);
            let mut t = -b - sq;
            if t < 1e-3 { t = -b + sq; }
            if t > 1e-3 && best.map_or(true, |(bt, _)| t < bt) { best = Some((t, i)); }
        }
    }
    best
}

fn trace(o: V, d: V, depth: u32) -> V {
    let light = V(-0.57735, 0.57735, -0.57735);
    match hit(o, d) {
        None => { let t = 0.5 * (d.1 + 1.0); V(1.0, 1.0, 1.0).mul(1.0 - t).add(V(0.5, 0.7, 1.0).mul(t)) }
        Some((t, i)) => {
            let s = &SPHERES[i];
            let p = o.add(d.mul(t));
            let n = p.sub(s.c).norm();
            let mut diff = n.dot(light);
            if diff < 0.0 { diff = 0.0; }
            if diff > 0.0 && hit(p, light).is_some() { diff *= 0.2; }
            let mut col = s.col.mul(0.1 + 0.9 * diff);
            if depth < 4 && s.refl > 0.0 {
                let r = d.sub(n.mul(2.0 * d.dot(n)));
                col = col.mul(1.0 - s.refl).add(trace(p, r, depth + 1).hadamard(V(s.refl, s.refl, s.refl)));
            }
            col
        }
    }
}

#[no_mangle]
pub extern "C" fn run(w: i32, h: i32) -> i64 {
    puts("raytrace\n");
    let mut hash = FNV0;
    let mut sum = 0.0f64;
    for y in 0..h {
        for x in 0..w {
            let u = (x as f32 + 0.5) / w as f32 * 2.0 - 1.0;
            let v = 1.0 - (y as f32 + 0.5) / h as f32 * 2.0;
            let d = V(u * (w as f32 / h as f32), v, 1.5).norm();
            let c = trace(V(0.0, 0.3, 0.0), d, 0);
            let px = [(c.0.min(1.0) * 255.0) as u8, (c.1.min(1.0) * 255.0) as u8, (c.2.min(1.0) * 255.0) as u8];
            hash = fnv(hash, &px);
            hash = fnv(hash, &c.0.to_bits().to_le_bytes());
            sum += (c.0 + c.1 + c.2) as f64;
        }
    }
    pf64(sum);
    pi64(hash as i64);
    hash as i64
}
