//! Deterministic randomness: RNG, value noise, and blue-noise point sampling.

use bevy::prelude::*;

/// SplitMix64: small, fast, and good enough for placement.
#[derive(Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed ^ 0x9E37_79B9_7F4A_7C15)
    }

    pub fn fork(&mut self, salt: u64) -> Self {
        Self::new(self.next_u64() ^ salt.wrapping_mul(0xD1B5_4A32_D192_ED03))
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in [0, 1).
    pub fn f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    pub fn range(&mut self, range: std::ops::Range<f32>) -> f32 {
        range.start + (range.end - range.start) * self.f32()
    }

    pub fn index(&mut self, len: usize) -> usize {
        ((self.next_u64() >> 33) as usize) % len.max(1)
    }

    pub fn chance(&mut self, p: f32) -> bool {
        self.f32() < p
    }

    pub fn angle(&mut self) -> f32 {
        self.f32() * std::f32::consts::TAU
    }

    /// Approximately normal (Irwin–Hall with 4 samples), mean 0, std ~1.
    pub fn normal(&mut self) -> f32 {
        (self.f32() + self.f32() + self.f32() + self.f32() - 2.0) * 1.732
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.index(items.len())]
    }
}

fn hash2(x: i32, y: i32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x8DA6_B343)
        ^ (y as u32).wrapping_mul(0xD816_3841)
        ^ seed.wrapping_mul(0xCB1A_B31F);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5BD1_E995);
    h ^= h >> 15;
    (h & 0x00FF_FFFF) as f32 / 0x00FF_FFFF as f32 * 2.0 - 1.0
}

/// Smooth value noise in [-1, 1].
pub fn value_noise(p: Vec2, seed: u32) -> f32 {
    let i = p.floor();
    let f = p - i;
    let u = f * f * (Vec2::splat(3.0) - 2.0 * f);
    let (x, y) = (i.x as i32, i.y as i32);
    let a = hash2(x, y, seed);
    let b = hash2(x + 1, y, seed);
    let c = hash2(x, y + 1, seed);
    let d = hash2(x + 1, y + 1, seed);
    a + (b - a) * u.x + (c - a) * u.y + (a - b - c + d) * u.x * u.y
}

/// Fractal noise in roughly [-1, 1].
pub fn fbm(p: Vec2, octaves: u32, seed: u32) -> f32 {
    let mut sum = 0.0;
    let mut amp = 0.5;
    let mut freq = 1.0;
    let mut norm = 0.0;
    for o in 0..octaves {
        sum += amp * value_noise(p * freq + Vec2::splat(o as f32 * 17.3), seed.wrapping_add(o * 101));
        norm += amp;
        amp *= 0.5;
        freq *= 2.03;
    }
    sum / norm
}

pub fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Bridson Poisson-disk sampling over a rectangle with a fixed minimum spacing.
/// Callers thin the result with a density field to get variable spacing.
pub fn poisson_disk(min: Vec2, max: Vec2, radius: f32, rng: &mut Rng) -> Vec<Vec2> {
    const ATTEMPTS: usize = 20;
    let cell = radius / std::f32::consts::SQRT_2;
    let size = max - min;
    let gw = (size.x / cell).ceil() as usize + 1;
    let gh = (size.y / cell).ceil() as usize + 1;
    let mut grid: Vec<u32> = vec![u32::MAX; gw * gh];
    let mut points: Vec<Vec2> = Vec::new();
    let mut active: Vec<usize> = Vec::new();

    let cell_of = |p: Vec2| -> (usize, usize) {
        (((p.x - min.x) / cell) as usize, ((p.y - min.y) / cell) as usize)
    };

    let first = min + Vec2::new(rng.f32() * size.x, rng.f32() * size.y);
    let (cx, cy) = cell_of(first);
    grid[cy * gw + cx] = 0;
    points.push(first);
    active.push(0);

    while !active.is_empty() {
        let slot = rng.index(active.len());
        let base = points[active[slot]];
        let mut found = false;
        for _ in 0..ATTEMPTS {
            let angle = rng.angle();
            let dist = radius * (1.0 + rng.f32());
            let candidate = base + Vec2::from_angle(angle) * dist;
            if candidate.x < min.x || candidate.y < min.y || candidate.x >= max.x || candidate.y >= max.y {
                continue;
            }
            let (cx, cy) = cell_of(candidate);
            let mut ok = true;
            'search: for y in cy.saturating_sub(2)..(cy + 3).min(gh) {
                for x in cx.saturating_sub(2)..(cx + 3).min(gw) {
                    let idx = grid[y * gw + x];
                    if idx != u32::MAX && points[idx as usize].distance_squared(candidate) < radius * radius {
                        ok = false;
                        break 'search;
                    }
                }
            }
            if ok {
                grid[cy * gw + cx] = points.len() as u32;
                active.push(points.len());
                points.push(candidate);
                found = true;
                break;
            }
        }
        if !found {
            active.swap_remove(slot);
        }
    }
    points
}

/// Uniform 2D spatial hash for "is anything within r of p" queries.
pub struct SpatialGrid {
    cell: f32,
    cells: std::collections::HashMap<(i32, i32), Vec<(Vec2, f32)>>,
}

impl SpatialGrid {
    pub fn new(cell: f32) -> Self {
        Self { cell, cells: Default::default() }
    }

    fn key(&self, p: Vec2) -> (i32, i32) {
        ((p.x / self.cell).floor() as i32, (p.y / self.cell).floor() as i32)
    }

    pub fn insert(&mut self, p: Vec2, radius: f32) {
        let key = self.key(p);
        self.cells.entry(key).or_default().push((p, radius));
    }

    /// True if p (with its own radius) overlaps any inserted disc.
    pub fn overlaps(&self, p: Vec2, radius: f32) -> bool {
        let (kx, ky) = self.key(p);
        let reach = ((radius + self.cell) / self.cell).ceil() as i32 + 1;
        for y in ky - reach..=ky + reach {
            for x in kx - reach..=kx + reach {
                if let Some(items) = self.cells.get(&(x, y)) {
                    for (q, r) in items {
                        if q.distance_squared(p) < (r + radius) * (r + radius) {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    /// Sum of `f(distance, radius)` over discs within `reach` of p.
    pub fn accumulate(&self, p: Vec2, reach: f32, mut f: impl FnMut(f32, f32) -> f32) -> f32 {
        let (kx, ky) = self.key(p);
        let n = (reach / self.cell).ceil() as i32;
        let mut sum = 0.0;
        for y in ky - n..=ky + n {
            for x in kx - n..=kx + n {
                if let Some(items) = self.cells.get(&(x, y)) {
                    for (q, r) in items {
                        let d = q.distance(p);
                        if d < reach {
                            sum += f(d, *r);
                        }
                    }
                }
            }
        }
        sum
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poisson_points_respect_spacing() {
        let mut rng = Rng::new(7);
        let points = poisson_disk(Vec2::ZERO, Vec2::splat(60.0), 4.0, &mut rng);
        assert!(points.len() > 80);
        for (i, a) in points.iter().enumerate() {
            for b in &points[i + 1..] {
                assert!(a.distance(*b) >= 4.0 - 1e-3);
            }
        }
    }

    #[test]
    fn noise_is_bounded_and_deterministic() {
        for i in 0..500 {
            let p = Vec2::new(i as f32 * 0.37, i as f32 * -0.21);
            let n = fbm(p, 4, 3);
            assert!((-1.0..=1.0).contains(&n));
            assert_eq!(n, fbm(p, 4, 3));
        }
    }
}
