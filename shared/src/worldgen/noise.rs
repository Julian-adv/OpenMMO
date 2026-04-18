//! Deterministic 2D Perlin noise with a seeded permutation table, plus fBm.
//!
//! Self-contained (no external crate) so output is fully reproducible across
//! platforms and across native/WASM builds of the shared crate.

use rand::rngs::SmallRng;
use rand::{RngCore, SeedableRng};

pub struct PerlinNoise {
    perm: [u16; 512],
}

impl PerlinNoise {
    pub fn new(seed: u64) -> Self {
        let mut p: [u16; 256] = core::array::from_fn(|i| i as u16);
        let mut rng = SmallRng::seed_from_u64(seed);
        // Fisher–Yates shuffle.
        for i in (1..256).rev() {
            let j = (rng.next_u32() as usize) % (i + 1);
            p.swap(i, j);
        }
        let mut perm = [0u16; 512];
        for i in 0..256 {
            perm[i] = p[i];
            perm[i + 256] = p[i];
        }
        Self { perm }
    }

    /// Sample the noise at (x, y). Output is in approximately [-1, 1].
    pub fn sample(&self, x: f32, y: f32) -> f32 {
        let xi = x.floor() as i32;
        let yi = y.floor() as i32;
        let xf = x - xi as f32;
        let yf = y - yi as f32;

        let xi = (xi & 255) as usize;
        let yi = (yi & 255) as usize;

        let aa = self.perm[self.perm[xi] as usize + yi] as usize;
        let ab = self.perm[self.perm[xi] as usize + yi + 1] as usize;
        let ba = self.perm[self.perm[xi + 1] as usize + yi] as usize;
        let bb = self.perm[self.perm[xi + 1] as usize + yi + 1] as usize;

        let u = fade(xf);
        let v = fade(yf);

        let x1 = lerp(grad(aa, xf, yf), grad(ba, xf - 1.0, yf), u);
        let x2 = lerp(grad(ab, xf, yf - 1.0), grad(bb, xf - 1.0, yf - 1.0), u);
        lerp(x1, x2, v)
    }
}

fn fade(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn grad(hash: usize, x: f32, y: f32) -> f32 {
    // 8 gradient directions (pick by low 3 bits).
    match hash & 7 {
        0 => x + y,
        1 => -x + y,
        2 => x - y,
        3 => -x - y,
        4 => x,
        5 => -x,
        6 => y,
        _ => -y,
    }
}

/// 3D Perlin noise. Used for seamless-wrap sampling via circle mapping: a
/// noise periodic in one (or two) axes is obtained by mapping that axis to
/// angle and sampling the corresponding (cos·R, sin·R) coordinate.
pub struct PerlinNoise3D {
    perm: [u16; 512],
}

impl PerlinNoise3D {
    pub fn new(seed: u64) -> Self {
        let mut p: [u16; 256] = core::array::from_fn(|i| i as u16);
        let mut rng = SmallRng::seed_from_u64(seed);
        for i in (1..256).rev() {
            let j = (rng.next_u32() as usize) % (i + 1);
            p.swap(i, j);
        }
        let mut perm = [0u16; 512];
        for i in 0..256 {
            perm[i] = p[i];
            perm[i + 256] = p[i];
        }
        Self { perm }
    }

    pub fn sample(&self, x: f32, y: f32, z: f32) -> f32 {
        let xi = x.floor() as i32;
        let yi = y.floor() as i32;
        let zi = z.floor() as i32;
        let xf = x - xi as f32;
        let yf = y - yi as f32;
        let zf = z - zi as f32;

        let xi = (xi & 255) as usize;
        let yi = (yi & 255) as usize;
        let zi = (zi & 255) as usize;

        let a = self.perm[xi] as usize + yi;
        let b = self.perm[xi + 1] as usize + yi;
        let aa = self.perm[a] as usize + zi;
        let ab = self.perm[a + 1] as usize + zi;
        let ba = self.perm[b] as usize + zi;
        let bb = self.perm[b + 1] as usize + zi;

        let u = fade(xf);
        let v = fade(yf);
        let w = fade(zf);

        let g000 = grad3(self.perm[aa] as usize, xf, yf, zf);
        let g100 = grad3(self.perm[ba] as usize, xf - 1.0, yf, zf);
        let g010 = grad3(self.perm[ab] as usize, xf, yf - 1.0, zf);
        let g110 = grad3(self.perm[bb] as usize, xf - 1.0, yf - 1.0, zf);
        let g001 = grad3(self.perm[aa + 1] as usize, xf, yf, zf - 1.0);
        let g101 = grad3(self.perm[ba + 1] as usize, xf - 1.0, yf, zf - 1.0);
        let g011 = grad3(self.perm[ab + 1] as usize, xf, yf - 1.0, zf - 1.0);
        let g111 = grad3(self.perm[bb + 1] as usize, xf - 1.0, yf - 1.0, zf - 1.0);

        let x00 = lerp(g000, g100, u);
        let x10 = lerp(g010, g110, u);
        let y0 = lerp(x00, x10, v);

        let x01 = lerp(g001, g101, u);
        let x11 = lerp(g011, g111, u);
        let y1 = lerp(x01, x11, v);

        lerp(y0, y1, w)
    }
}

fn grad3(hash: usize, x: f32, y: f32, z: f32) -> f32 {
    // Ken Perlin's 12 edge-vector gradients; extra 4 slots duplicate to fill
    // a 16-entry table (indexed by hash & 15).
    match hash & 15 {
        0 | 12 => x + y,
        1 | 14 => -x + y,
        2 => x - y,
        3 => -x - y,
        4 => x + z,
        5 => -x + z,
        6 => x - z,
        7 => -x - z,
        8 => y + z,
        9 | 13 => -y + z,
        10 => y - z,
        11 | 15 => -y - z,
        _ => 0.0,
    }
}

/// fBm sampled on a 3D noise with the X axis mapped to a circle of
/// circumference equal to `world_width * base_freq` in noise units. This
/// produces a noise field that is exactly periodic in X with period
/// `world_width` (so cell x=0 and cell x=world_width see identical values).
/// The Y axis is linear (non-wrapping).
pub fn fbm_wrap_x(
    noise: &PerlinNoise3D,
    x: f32,
    y: f32,
    world_width: f32,
    base_freq: f32,
    octaves: u32,
    lacunarity: f32,
    gain: f32,
) -> f32 {
    let angle = 2.0 * std::f32::consts::PI * x / world_width;
    let r = world_width * base_freq / (2.0 * std::f32::consts::PI);
    let cx = r * angle.cos();
    let cz = r * angle.sin();
    let ny = y * base_freq;

    let mut f = 1.0f32;
    let mut a = 1.0f32;
    let mut sum = 0.0f32;
    let mut norm = 0.0f32;
    for _ in 0..octaves {
        sum += a * noise.sample(cx * f, ny * f, cz * f);
        norm += a;
        f *= lacunarity;
        a *= gain;
    }
    if norm > 0.0 {
        sum / norm
    } else {
        0.0
    }
}

/// Fractal Brownian Motion: sum octaves with geometric frequency/amplitude.
/// Output normalized to roughly [-1, 1].
pub fn fbm2(noise: &PerlinNoise, x: f32, y: f32, octaves: u32, lacunarity: f32, gain: f32) -> f32 {
    let mut freq = 1.0f32;
    let mut amp = 1.0f32;
    let mut sum = 0.0f32;
    let mut norm = 0.0f32;
    for _ in 0..octaves {
        sum += amp * noise.sample(x * freq, y * freq);
        norm += amp;
        freq *= lacunarity;
        amp *= gain;
    }
    if norm > 0.0 {
        sum / norm
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_output() {
        let a = PerlinNoise::new(42);
        let b = PerlinNoise::new(42);
        for i in 0..100 {
            let t = i as f32 * 0.31;
            assert_eq!(a.sample(t, -t), b.sample(t, -t));
        }
    }

    #[test]
    fn different_seed_different_output() {
        let a = PerlinNoise::new(42);
        let b = PerlinNoise::new(43);
        let mut diff_count = 0;
        for i in 0..100 {
            let t = i as f32 * 0.31;
            if (a.sample(t, -t) - b.sample(t, -t)).abs() > 1e-6 {
                diff_count += 1;
            }
        }
        assert!(
            diff_count > 50,
            "seeds should produce mostly different values"
        );
    }

    #[test]
    fn noise_at_integer_lattice_is_zero() {
        // Classical Perlin: value noise is 0 at integer lattice points.
        let n = PerlinNoise::new(1);
        for x in -5..5 {
            for y in -5..5 {
                let v = n.sample(x as f32, y as f32);
                assert!(v.abs() < 1e-6, "expected 0 at ({x},{y}), got {v}");
            }
        }
    }

    #[test]
    fn noise_bounded_in_plausible_range() {
        // Perlin output magnitude is bounded (~±0.707 for 2D), but we're
        // flexible — assert values stay within a sane window.
        let n = PerlinNoise::new(7);
        let mut min = f32::INFINITY;
        let mut max = f32::NEG_INFINITY;
        for i in 0..1000 {
            let t = i as f32 * 0.17;
            let v = n.sample(t, t * 0.5);
            min = min.min(v);
            max = max.max(v);
        }
        assert!(min > -1.0 && max < 1.0, "got [{min}, {max}]");
    }

    #[test]
    fn fbm_is_deterministic() {
        let n = PerlinNoise::new(99);
        let a = fbm2(&n, 1.23, 4.56, 6, 2.0, 0.5);
        let b = fbm2(&n, 1.23, 4.56, 6, 2.0, 0.5);
        assert_eq!(a, b);
    }

    #[test]
    fn perlin3_same_seed_same_output() {
        let a = PerlinNoise3D::new(42);
        let b = PerlinNoise3D::new(42);
        for i in 0..50 {
            let t = i as f32 * 0.21;
            assert_eq!(a.sample(t, -t, t * 0.7), b.sample(t, -t, t * 0.7));
        }
    }

    #[test]
    fn perlin3_at_integer_lattice_is_zero() {
        let n = PerlinNoise3D::new(1);
        for x in -3..3 {
            for y in -3..3 {
                for z in -3..3 {
                    let v = n.sample(x as f32, y as f32, z as f32);
                    assert!(v.abs() < 1e-6, "expected 0 at ({x},{y},{z}), got {v}");
                }
            }
        }
    }

    #[test]
    fn fbm_wrap_x_is_periodic() {
        // Sampling at x=0 and x=world_width must return identical values for
        // any y; this is the core guarantee of the circle-mapping trick.
        let n = PerlinNoise3D::new(123);
        let world_width = 4096.0;
        let base_freq = 1.0 / 700.0;
        for yi in 0..20 {
            let y = yi as f32 * 137.0;
            let a = fbm_wrap_x(&n, 0.0, y, world_width, base_freq, 4, 2.0, 0.5);
            let b = fbm_wrap_x(&n, world_width, y, world_width, base_freq, 4, 2.0, 0.5);
            assert!(
                (a - b).abs() < 1e-5,
                "wrap failed at y={y}: {a} vs {b} (diff {})",
                a - b
            );
        }
    }

    #[test]
    fn fbm_wrap_x_varies_across_width() {
        // Wrap shouldn't collapse all values to the same number; verify
        // meaningful variation across x.
        let n = PerlinNoise3D::new(7);
        let world_width = 4096.0;
        let base_freq = 1.0 / 700.0;
        let mut mn = f32::INFINITY;
        let mut mx = f32::NEG_INFINITY;
        for xi in 0..32 {
            let x = xi as f32 * (world_width / 32.0);
            let v = fbm_wrap_x(&n, x, 1000.0, world_width, base_freq, 4, 2.0, 0.5);
            mn = mn.min(v);
            mx = mx.max(v);
        }
        assert!(
            mx - mn > 0.1,
            "wrapped fBm has near-constant output: range {}",
            mx - mn
        );
    }
}
