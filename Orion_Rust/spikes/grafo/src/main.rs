//! Spike F00.16 (parte CPU): layout de forças 3D, O(n²) contra Barnes-Hut (octree), 1 thread e N threads.
//! Mede ms por iteração em 1.000, 5.000 e 10.000 nós (arestas ≈ 3n). A parte de compute shader exige GPU.
use rayon::prelude::*;
use std::time::Instant;

#[derive(Clone, Copy, Default)]
struct V([f32; 3]);
impl V {
    fn add(self, o: V) -> V { V([self.0[0] + o.0[0], self.0[1] + o.0[1], self.0[2] + o.0[2]]) }
    fn sub(self, o: V) -> V { V([self.0[0] - o.0[0], self.0[1] - o.0[1], self.0[2] - o.0[2]]) }
    fn mul(self, k: f32) -> V { V([self.0[0] * k, self.0[1] * k, self.0[2] * k]) }
    fn len2(self) -> f32 { self.0[0] * self.0[0] + self.0[1] * self.0[1] + self.0[2] * self.0[2] }
}

struct Rng(u64);
impl Rng { fn f(&mut self) -> f32 { self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407); ((self.0 >> 33) as f32 / (1u64 << 31) as f32) - 0.5 } }

struct Node { c: V, half: f32, com: V, mass: f32, kids: [i32; 8], body: i32 }

fn octant(c: V, p: V) -> usize { ((p.0[0] > c.0[0]) as usize) | (((p.0[1] > c.0[1]) as usize) << 1) | (((p.0[2] > c.0[2]) as usize) << 2) }

fn build(pos: &[V]) -> Vec<Node> {
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for p in pos { for k in 0..3 { lo[k] = lo[k].min(p.0[k]); hi[k] = hi[k].max(p.0[k]); } }
    let c = V([(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0]);
    let half = (0..3).map(|k| hi[k] - lo[k]).fold(0.0, f32::max) / 2.0 + 1e-3;
    let mut t = vec![Node { c, half, com: V::default(), mass: 0.0, kids: [-1; 8], body: -1 }];
    for (i, &p) in pos.iter().enumerate() { insert(&mut t, 0, i as i32, p, pos, 0); }
    t
}

fn insert(t: &mut Vec<Node>, n: usize, i: i32, p: V, pos: &[V], depth: u32) {
    let (m, cm) = (t[n].mass, t[n].com);
    t[n].com = cm.mul(m).add(p).mul(1.0 / (m + 1.0));
    t[n].mass = m + 1.0;
    if m == 0.0 { t[n].body = i; return; }
    if depth > 40 { return; } // pontos coincidentes
    if t[n].body >= 0 { // folha ocupada: empurra o corpo antigo para um filho
        let old = t[n].body; t[n].body = -1;
        child_insert(t, n, old, pos[old as usize], pos, depth);
    }
    child_insert(t, n, i, p, pos, depth);
}

fn child_insert(t: &mut Vec<Node>, n: usize, i: i32, p: V, pos: &[V], depth: u32) {
    let o = octant(t[n].c, p);
    if t[n].kids[o] < 0 {
        let h = t[n].half / 2.0;
        let c = V([t[n].c.0[0] + if o & 1 > 0 { h } else { -h }, t[n].c.0[1] + if o & 2 > 0 { h } else { -h }, t[n].c.0[2] + if o & 4 > 0 { h } else { -h }]);
        t.push(Node { c, half: h, com: V::default(), mass: 0.0, kids: [-1; 8], body: -1 });
        let idx = (t.len() - 1) as i32;
        t[n].kids[o] = idx;
    }
    let k = t[n].kids[o] as usize;
    insert(t, k, i, p, pos, depth + 1);
}

fn force_bh(t: &[Node], n: usize, i: usize, p: V, theta2: f32, k: f32) -> V {
    let nd = &t[n];
    if nd.mass == 0.0 { return V::default(); }
    let d = p.sub(nd.com);
    let r2 = d.len2() + 0.01;
    if nd.body >= 0 || (nd.half * 2.0).powi(2) / r2 < theta2 {
        if nd.body == i as i32 { return V::default(); }
        return d.mul(k * nd.mass / (r2 * r2.sqrt()));
    }
    let mut f = V::default();
    for &c in &nd.kids { if c >= 0 { f = f.add(force_bh(t, c as usize, i, p, theta2, k)); } }
    f
}

fn step(pos: &mut [V], edges: &[(usize, usize)], mode: &str, k: f32) {
    let n = pos.len();
    let rep: Vec<V> = match mode {
        "n2" => (0..n).into_par_iter().map(|i| { let mut f = V::default(); for j in 0..n { if i != j { let d = pos[i].sub(pos[j]); let r2 = d.len2() + 0.01; f = f.add(d.mul(k / (r2 * r2.sqrt()))); } } f }).collect(),
        "bh" | "bh1" => { let t = build(pos); let g = |i: usize| force_bh(&t, 0, i, pos[i], 0.5 * 0.5, k); if mode == "bh" { (0..n).into_par_iter().map(g).collect() } else { (0..n).map(g).collect() } }
        _ => unreachable!(),
    };
    let mut f = rep;
    for &(a, b) in edges { let d = pos[b].sub(pos[a]).mul(0.02); f[a] = f[a].add(d); f[b] = f[b].sub(d); }
    for i in 0..n { pos[i] = pos[i].add(f[i].mul(0.5)).sub(pos[i].mul(0.001)); }
}

fn main() {
    println!("threads={}", rayon::current_num_threads());
    for &n in &[1000usize, 5000, 10000] {
        let mut r = Rng(7);
        let init: Vec<V> = (0..n).map(|_| V([r.f() * 50.0, r.f() * 50.0, r.f() * 50.0])).collect();
        let edges: Vec<(usize, usize)> = (0..n * 3).map(|_| { let a = (r.f().abs() * 2.0 * n as f32) as usize % n; let b = (r.f().abs() * 2.0 * n as f32) as usize % n; (a, b) }).filter(|(a, b)| a != b).collect();
        for mode in ["n2", "bh", "bh1"] {
            if mode == "n2" && n > 5000 { continue; }
            let mut pos = init.clone();
            let iters = if mode == "n2" && n >= 5000 { 3 } else { 10 };
            let t = Instant::now();
            for _ in 0..iters { step(&mut pos, &edges, mode, 20.0); }
            let ms = t.elapsed().as_secs_f64() * 1000.0 / iters as f64;
            println!("n={n:6} {mode:4} {ms:9.1} ms/iteração ({:.1} it/s)", 1000.0 / ms);
        }
    }
}
