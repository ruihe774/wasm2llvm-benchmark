//! n-body simulation and spectral norm (f64 heavy, benchmark-game style).
#![no_std]
#![feature(wasm_numeric_instr)]

#[path = "../../common/common.rs"]
mod common;
use common::*;
use core::arch::wasm32::f64_sqrt;

const PI: f64 = 3.141592653589793;
const SOLAR_MASS: f64 = 4.0 * PI * PI;
const DAYS: f64 = 365.24;

#[derive(Clone, Copy)]
struct Body { p: [f64; 3], v: [f64; 3], m: f64 }

fn bodies() -> [Body; 5] {
    [
        Body { p: [0.0; 3], v: [0.0; 3], m: SOLAR_MASS },
        Body { p: [4.84143144246472090e+00, -1.16032004402742839e+00, -1.03622044471123109e-01],
               v: [1.66007664274403694e-03 * DAYS, 7.69901118419740425e-03 * DAYS, -6.90460016972063023e-05 * DAYS],
               m: 9.54791938424326609e-04 * SOLAR_MASS },
        Body { p: [8.34336671824457987e+00, 4.12479856412430479e+00, -4.03523417114321381e-01],
               v: [-2.76742510726862411e-03 * DAYS, 4.99852801234917238e-03 * DAYS, 2.30417297573763929e-05 * DAYS],
               m: 2.85885980666130812e-04 * SOLAR_MASS },
        Body { p: [1.28943695621391310e+01, -1.51111514016986312e+01, -2.23307578892655734e-01],
               v: [2.96460137564761618e-03 * DAYS, 2.37847173959480950e-03 * DAYS, -2.96589568540237556e-05 * DAYS],
               m: 4.36624404335156298e-05 * SOLAR_MASS },
        Body { p: [1.53796971148509165e+01, -2.59193146099879641e+01, 1.79258772950371181e-01],
               v: [2.68067772490389322e-03 * DAYS, 1.62824170038242295e-03 * DAYS, -9.51592254519715870e-05 * DAYS],
               m: 5.15138902046611451e-05 * SOLAR_MASS },
    ]
}

fn energy(b: &[Body; 5]) -> f64 {
    let mut e = 0.0;
    for i in 0..5 {
        e += 0.5 * b[i].m * (b[i].v[0] * b[i].v[0] + b[i].v[1] * b[i].v[1] + b[i].v[2] * b[i].v[2]);
        for j in i + 1..5 {
            let d = [b[i].p[0] - b[j].p[0], b[i].p[1] - b[j].p[1], b[i].p[2] - b[j].p[2]];
            e -= b[i].m * b[j].m / f64_sqrt(d[0] * d[0] + d[1] * d[1] + d[2] * d[2]);
        }
    }
    e
}

fn advance(b: &mut [Body; 5], dt: f64) {
    for i in 0..5 {
        for j in i + 1..5 {
            let d = [b[i].p[0] - b[j].p[0], b[i].p[1] - b[j].p[1], b[i].p[2] - b[j].p[2]];
            let d2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
            let mag = dt / (d2 * f64_sqrt(d2));
            let (mi, mj) = (b[i].m, b[j].m);
            for k in 0..3 {
                b[i].v[k] -= d[k] * mj * mag;
                b[j].v[k] += d[k] * mi * mag;
            }
        }
    }
    for bi in b.iter_mut() {
        for k in 0..3 { bi.p[k] += dt * bi.v[k]; }
    }
}

fn a(i: usize, j: usize) -> f64 { 1.0 / ((i + j) * (i + j + 1) / 2 + i + 1) as f64 }

fn mul_av(v: &[f64], out: &mut [f64]) {
    for i in 0..out.len() { out[i] = v.iter().enumerate().map(|(j, x)| a(i, j) * x).sum(); }
}
fn mul_atv(v: &[f64], out: &mut [f64]) {
    for i in 0..out.len() { out[i] = v.iter().enumerate().map(|(j, x)| a(j, i) * x).sum(); }
}

static mut U: [f64; 400] = [1.0; 400];
static mut VV: [f64; 400] = [0.0; 400];
static mut T: [f64; 400] = [0.0; 400];

fn spectral(n: usize) -> f64 {
    let (u, v, t) = unsafe {
        (&mut (&mut *core::ptr::addr_of_mut!(U))[..n], &mut (&mut *core::ptr::addr_of_mut!(VV))[..n],
         &mut (&mut *core::ptr::addr_of_mut!(T))[..n])
    };
    for x in u.iter_mut() { *x = 1.0; }
    for _ in 0..10 {
        mul_av(u, t); mul_atv(t, v);
        mul_av(v, t); mul_atv(t, u);
    }
    let vbv: f64 = u.iter().zip(v.iter()).map(|(a, b)| a * b).sum();
    let vv: f64 = v.iter().map(|x| x * x).sum();
    f64_sqrt(vbv / vv)
}

#[no_mangle]
pub extern "C" fn run(steps: i32, n: i32) -> i64 {
    puts("nbody\n");
    let mut b = bodies();
    let mut px = 0.0; let mut py = 0.0; let mut pz = 0.0;
    for bi in b.iter() { px += bi.v[0] * bi.m; py += bi.v[1] * bi.m; pz += bi.v[2] * bi.m; }
    b[0].v = [-px / SOLAR_MASS, -py / SOLAR_MASS, -pz / SOLAR_MASS];
    let e0 = energy(&b);
    for _ in 0..steps { advance(&mut b, 0.01); }
    let e1 = energy(&b);
    pf64(e0);
    pf64(e1);
    let s = spectral((n as usize).min(400));
    pf64(s);
    (e1.to_bits() ^ s.to_bits()) as i64
}
