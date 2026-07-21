//! Deterministic 2D Perlin noise with a seeded permutation table, plus fBm.
//!
//! Self-contained (no external crate) so output is fully reproducible across
//! platforms and across native/WASM builds of the shared crate.

use rand::rngs::SmallRng;
use rand::{RngCore, SeedableRng};

/// Build a duplicated 512-entry permutation table from a seed via Fisher-Yates
/// shuffle. The duplication eliminates lookup index masking in noise samplers.
fn build_perm_table(seed: u64) -> [u16; 512] {
    let mut p: [u16; 256] = core::array::from_fn(|i| i as u16);
    let mut rng = SmallRng::seed_from_u64(seed);
    for i in (1..256).rev() {
        let j = (rng.next_u32() as usize) % (i + 1);
        p.swap(i, j);
    }
    let mut perm = [0u16; 512];
    perm[..256].copy_from_slice(&p);
    perm[256..].copy_from_slice(&p);
    perm
}

pub struct PerlinNoise {
    perm: [u16; 512],
}

impl PerlinNoise {
    pub fn new(seed: u64) -> Self {
        Self {
            perm: build_perm_table(seed),
        }
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

/// 4D Perlin noise. Used for seamless torus sampling: each wrapped axis maps
/// to a circle (Clifford torus embedding), so the field is exactly periodic
/// in both world axes.
pub struct PerlinNoise4D {
    perm: [u16; 512],
}

impl PerlinNoise4D {
    pub fn new(seed: u64) -> Self {
        Self {
            perm: build_perm_table(seed),
        }
    }

    pub fn sample(&self, x: f32, y: f32, z: f32, w: f32) -> f32 {
        let xi = x.floor() as i32;
        let yi = y.floor() as i32;
        let zi = z.floor() as i32;
        let wi = w.floor() as i32;
        let xf = x - xi as f32;
        let yf = y - yi as f32;
        let zf = z - zi as f32;
        let wf = w - wi as f32;

        let xi = (xi & 255) as usize;
        let yi = (yi & 255) as usize;
        let zi = (zi & 255) as usize;
        let wi = (wi & 255) as usize;

        let a = self.perm[xi] as usize + yi;
        let b = self.perm[xi + 1] as usize + yi;
        let aa = self.perm[a] as usize + zi;
        let ab = self.perm[a + 1] as usize + zi;
        let ba = self.perm[b] as usize + zi;
        let bb = self.perm[b + 1] as usize + zi;
        let aaa = self.perm[aa] as usize + wi;
        let aab = self.perm[aa + 1] as usize + wi;
        let aba = self.perm[ab] as usize + wi;
        let abb = self.perm[ab + 1] as usize + wi;
        let baa = self.perm[ba] as usize + wi;
        let bab = self.perm[ba + 1] as usize + wi;
        let bba = self.perm[bb] as usize + wi;
        let bbb = self.perm[bb + 1] as usize + wi;

        let u = fade(xf);
        let v = fade(yf);
        let s = fade(zf);
        let t = fade(wf);

        let (x0, y0, z0, w0) = (xf, yf, zf, wf);
        let (x1, y1, z1, w1) = (xf - 1.0, yf - 1.0, zf - 1.0, wf - 1.0);

        // 16 corners, quadrilinear blend.
        let c0000 = grad4(self.perm[aaa] as usize, x0, y0, z0, w0);
        let c1000 = grad4(self.perm[baa] as usize, x1, y0, z0, w0);
        let c0100 = grad4(self.perm[aba] as usize, x0, y1, z0, w0);
        let c1100 = grad4(self.perm[bba] as usize, x1, y1, z0, w0);
        let c0010 = grad4(self.perm[aab] as usize, x0, y0, z1, w0);
        let c1010 = grad4(self.perm[bab] as usize, x1, y0, z1, w0);
        let c0110 = grad4(self.perm[abb] as usize, x0, y1, z1, w0);
        let c1110 = grad4(self.perm[bbb] as usize, x1, y1, z1, w0);
        let c0001 = grad4(self.perm[aaa + 1] as usize, x0, y0, z0, w1);
        let c1001 = grad4(self.perm[baa + 1] as usize, x1, y0, z0, w1);
        let c0101 = grad4(self.perm[aba + 1] as usize, x0, y1, z0, w1);
        let c1101 = grad4(self.perm[bba + 1] as usize, x1, y1, z0, w1);
        let c0011 = grad4(self.perm[aab + 1] as usize, x0, y0, z1, w1);
        let c1011 = grad4(self.perm[bab + 1] as usize, x1, y0, z1, w1);
        let c0111 = grad4(self.perm[abb + 1] as usize, x0, y1, z1, w1);
        let c1111 = grad4(self.perm[bbb + 1] as usize, x1, y1, z1, w1);

        let w0_blend = lerp(
            lerp(lerp(c0000, c1000, u), lerp(c0100, c1100, u), v),
            lerp(lerp(c0010, c1010, u), lerp(c0110, c1110, u), v),
            s,
        );
        let w1_blend = lerp(
            lerp(lerp(c0001, c1001, u), lerp(c0101, c1101, u), v),
            lerp(lerp(c0011, c1011, u), lerp(c0111, c1111, u), v),
            s,
        );
        lerp(w0_blend, w1_blend, t)
    }
}

fn grad4(hash: usize, x: f32, y: f32, z: f32, w: f32) -> f32 {
    // 32 gradients: drop one of the 4 coords (bits 3-4), ±1 signs on the
    // remaining three (bits 0-2).
    let h = hash & 31;
    let (a, b, c) = match h >> 3 {
        0 => (y, z, w),
        1 => (x, z, w),
        2 => (x, y, w),
        _ => (x, y, z),
    };
    let s0 = if h & 1 == 0 { a } else { -a };
    let s1 = if h & 2 == 0 { b } else { -b };
    let s2 = if h & 4 == 0 { c } else { -c };
    s0 + s1 + s2
}

/// fBm on 4D noise with both world axes mapped to circles (Clifford torus).
/// The field is exactly periodic in X and Y with period `world_size`. Circle
/// radius keeps arc length equal to planar noise distance, so feature scale
/// matches a non-wrapped fBm at the same `base_freq`.
#[allow(clippy::too_many_arguments)]
pub fn fbm_wrap_xy(
    noise: &PerlinNoise4D,
    x: f32,
    y: f32,
    world_size: f32,
    base_freq: f32,
    octaves: u32,
    lacunarity: f32,
    gain: f32,
) -> f32 {
    let (cx, cy, cz, cw) = wrap_xy_to_torus(x, y, world_size, base_freq);

    let mut f = 1.0f32;
    let mut a = 1.0f32;
    let mut sum = 0.0f32;
    let mut norm = 0.0f32;
    for _ in 0..octaves {
        sum += a * noise.sample(cx * f, cy * f, cz * f, cw * f);
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

/// Map world `(x, y)` onto the 4D Clifford-torus parameterization used by
/// `fbm_wrap_xy` and `fbm_wrap_xy_damped`: each axis folds to a circle of
/// radius `world_size·base_freq / 2π`.
#[inline]
fn wrap_xy_to_torus(x: f32, y: f32, world_size: f32, base_freq: f32) -> (f32, f32, f32, f32) {
    let tau = 2.0 * std::f32::consts::PI;
    let ax = tau * x / world_size;
    let ay = tau * y / world_size;
    let r = world_size * base_freq / tau;
    (r * ax.cos(), r * ax.sin(), r * ay.cos(), r * ay.sin())
}

/// Derivative of the quintic fade `f(t)=6t⁵-15t⁴+10t³`.
#[inline]
fn fade_deriv(t: f32) -> f32 {
    30.0 * t * t * (t * (t - 2.0) + 1.0)
}

/// 4D value noise with analytical gradient, for the damped-fBm pattern on
/// the torus parameterization. Internally interpolates two 3D slices (w=0,
/// w=1 lattice planes) and fade-blends them along the 4th axis, so the
/// closed-form 3D gradient math is reused instead of expanding all 16
/// multilinear terms.
pub struct ValueNoise4D {
    perm: [u16; 512],
}

impl ValueNoise4D {
    pub fn new(seed: u64) -> Self {
        Self {
            perm: build_perm_table(seed),
        }
    }

    /// Hash 4 lattice indices (each masked to 0..256) into a value in [-1, 1].
    #[inline]
    fn hash(&self, ix: usize, iy: usize, iz: usize, iw: usize) -> f32 {
        let h = self.perm
            [self.perm[self.perm[self.perm[ix] as usize + iy] as usize + iz] as usize + iw]
            as f32;
        h * (2.0 / 255.0) - 1.0
    }

    /// Value + gradient of one 3D slice at lattice w-index `iw`.
    #[allow(clippy::too_many_arguments)]
    #[inline]
    fn slice_with_deriv(
        &self,
        ix0: usize,
        iy0: usize,
        iz0: usize,
        iw: usize,
        u: f32,
        v: f32,
        w: f32,
        du: f32,
        dv: f32,
        dw: f32,
    ) -> (f32, f32, f32, f32) {
        let ix1 = (ix0 + 1) & 255;
        let iy1 = (iy0 + 1) & 255;
        let iz1 = (iz0 + 1) & 255;

        let a = self.hash(ix0, iy0, iz0, iw);
        let b = self.hash(ix1, iy0, iz0, iw);
        let c = self.hash(ix0, iy1, iz0, iw);
        let d = self.hash(ix1, iy1, iz0, iw);
        let e = self.hash(ix0, iy0, iz1, iw);
        let f = self.hash(ix1, iy0, iz1, iw);
        let g = self.hash(ix0, iy1, iz1, iw);
        let h = self.hash(ix1, iy1, iz1, iw);

        let k0 = a;
        let k1 = b - a;
        let k2 = c - a;
        let k3 = e - a;
        let k4 = a - b - c + d;
        let k5 = a - c - e + g;
        let k6 = a - b - e + f;
        let k7 = -a + b + c - d + e - f - g + h;

        let value =
            k0 + k1 * u + k2 * v + k3 * w + k4 * u * v + k5 * v * w + k6 * u * w + k7 * u * v * w;
        let dvdx = du * (k1 + k4 * v + k6 * w + k7 * v * w);
        let dvdy = dv * (k2 + k4 * u + k5 * w + k7 * u * w);
        let dvdz = dw * (k3 + k5 * v + k6 * u + k7 * u * v);
        (value, dvdx, dvdy, dvdz)
    }

    /// Sample at `(x, y, z, w)`. Returns `(value, d/dx, d/dy, d/dz, d/dw)`.
    pub fn sample_with_deriv(&self, x: f32, y: f32, z: f32, w: f32) -> (f32, f32, f32, f32, f32) {
        let xi = x.floor() as i32;
        let yi = y.floor() as i32;
        let zi = z.floor() as i32;
        let wi = w.floor() as i32;
        let fx = x - xi as f32;
        let fy = y - yi as f32;
        let fz = z - zi as f32;
        let fw = w - wi as f32;

        let ix0 = (xi & 255) as usize;
        let iy0 = (yi & 255) as usize;
        let iz0 = (zi & 255) as usize;
        let iw0 = (wi & 255) as usize;
        let iw1 = (iw0 + 1) & 255;

        let u = fade(fx);
        let v = fade(fy);
        let s = fade(fz);
        let t = fade(fw);
        let du = fade_deriv(fx);
        let dv = fade_deriv(fy);
        let ds = fade_deriv(fz);
        let dt = fade_deriv(fw);

        let (v0, dx0, dy0, dz0) = self.slice_with_deriv(ix0, iy0, iz0, iw0, u, v, s, du, dv, ds);
        let (v1, dx1, dy1, dz1) = self.slice_with_deriv(ix0, iy0, iz0, iw1, u, v, s, du, dv, ds);

        let value = lerp(v0, v1, t);
        let dvdx = lerp(dx0, dx1, t);
        let dvdy = lerp(dy0, dy1, t);
        let dvdz = lerp(dz0, dz1, t);
        let dvdw = (v1 - v0) * dt;
        (value, dvdx, dvdy, dvdz, dvdw)
    }
}

/// Derivative-damped fBm on the Clifford-torus parameterization (Iñigo
/// Quílez "morenoise"). Each octave's contribution is divided by
/// `1 + |Σ∇noise|²`, so further detail is damped wherever the surface is
/// already steep — yielding eroded ridges and smooth basins instead of
/// uniformly noisy fBm. Output rescaled to roughly [-1, 1] to match
/// `fbm_wrap_xy`'s contract. Periodic in both world axes.
#[allow(clippy::too_many_arguments)]
pub fn fbm_wrap_xy_damped(
    noise: &ValueNoise4D,
    x: f32,
    y: f32,
    world_size: f32,
    base_freq: f32,
    octaves: u32,
    lacunarity: f32,
    gain: f32,
) -> f32 {
    let (cx, cy, cz, cw) = wrap_xy_to_torus(x, y, world_size, base_freq);

    let mut f = 1.0f32;
    let mut amp = 1.0f32;
    let mut sum = 0.0f32;
    let (mut dx, mut dy, mut dz, mut dw) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    for _ in 0..octaves {
        let (n, ndx, ndy, ndz, ndw) = noise.sample_with_deriv(cx * f, cy * f, cz * f, cw * f);
        dx += ndx;
        dy += ndy;
        dz += ndz;
        dw += ndw;
        sum += amp * n / (1.0 + dx * dx + dy * dy + dz * dz + dw * dw);
        f *= lacunarity;
        amp *= gain;
    }
    // Damped fBm converges to ~half the amplitude of normalized fBm; the 2.0
    // gain spreads typical output back to ~[-1, 1] without a true running norm.
    (sum * 2.0).clamp(-1.0, 1.0)
}

/// Hermite-interpolated smoothstep. Returns 0 at `edge0`, 1 at `edge1`, with
/// a C¹-continuous ramp between. Works with inverted edges (edge0 > edge1).
#[inline]
pub fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
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
    fn perlin4_same_seed_same_output() {
        let a = PerlinNoise4D::new(42);
        let b = PerlinNoise4D::new(42);
        for i in 0..50 {
            let t = i as f32 * 0.21;
            assert_eq!(
                a.sample(t, -t, t * 0.7, t * 0.3),
                b.sample(t, -t, t * 0.7, t * 0.3)
            );
        }
    }

    #[test]
    fn perlin4_at_integer_lattice_is_zero() {
        let n = PerlinNoise4D::new(1);
        for x in -2..2 {
            for y in -2..2 {
                for z in -2..2 {
                    for w in -2..2 {
                        let v = n.sample(x as f32, y as f32, z as f32, w as f32);
                        assert!(v.abs() < 1e-6, "expected 0 at ({x},{y},{z},{w}), got {v}");
                    }
                }
            }
        }
    }

    #[test]
    fn fbm_wrap_xy_is_periodic_in_both_axes() {
        // Sampling at 0 and world_size must return identical values along
        // either axis; this is the core guarantee of the torus embedding.
        let n = PerlinNoise4D::new(123);
        let world_size = 4096.0;
        let base_freq = 1.0 / 700.0;
        for i in 0..20 {
            let t = i as f32 * 137.0;
            let a = fbm_wrap_xy(&n, 0.0, t, world_size, base_freq, 4, 2.0, 0.5);
            let b = fbm_wrap_xy(&n, world_size, t, world_size, base_freq, 4, 2.0, 0.5);
            assert!((a - b).abs() < 1e-5, "x-wrap failed at y={t}: {a} vs {b}");
            let c = fbm_wrap_xy(&n, t, 0.0, world_size, base_freq, 4, 2.0, 0.5);
            let d = fbm_wrap_xy(&n, t, world_size, world_size, base_freq, 4, 2.0, 0.5);
            assert!((c - d).abs() < 1e-5, "y-wrap failed at x={t}: {c} vs {d}");
        }
    }

    #[test]
    fn fbm_wrap_xy_varies_across_both_axes() {
        // Wrap shouldn't collapse all values to the same number; verify
        // meaningful variation along each axis independently.
        let n = PerlinNoise4D::new(7);
        let world_size = 4096.0;
        let base_freq = 1.0 / 700.0;
        for axis in 0..2 {
            let mut mn = f32::INFINITY;
            let mut mx = f32::NEG_INFINITY;
            for i in 0..32 {
                let t = i as f32 * (world_size / 32.0);
                let (x, y) = if axis == 0 { (t, 1000.0) } else { (1000.0, t) };
                let v = fbm_wrap_xy(&n, x, y, world_size, base_freq, 4, 2.0, 0.5);
                mn = mn.min(v);
                mx = mx.max(v);
            }
            assert!(
                mx - mn > 0.1,
                "wrapped fBm near-constant along axis {axis}: range {}",
                mx - mn
            );
        }
    }

    #[test]
    fn value_noise4_analytic_derivative_matches_central_difference() {
        let n = ValueNoise4D::new(31);
        let h = 1e-3f32;
        for &(x, y, z, w) in &[
            (0.37f32, 1.21, -0.55, 0.8),
            (-2.4, 3.8, 0.2, -1.3),
            (5.5, 0.0, 2.1, 4.2),
        ] {
            let (_, dx, dy, dz, dw) = n.sample_with_deriv(x, y, z, w);
            let cd = |f: &dyn Fn(f32) -> f32| (f(h) - f(-h)) / (2.0 * h);
            let cdx = cd(&|e| n.sample_with_deriv(x + e, y, z, w).0);
            let cdy = cd(&|e| n.sample_with_deriv(x, y + e, z, w).0);
            let cdz = cd(&|e| n.sample_with_deriv(x, y, z + e, w).0);
            let cdw = cd(&|e| n.sample_with_deriv(x, y, z, w + e).0);
            for (name, a, b) in [
                ("dx", dx, cdx),
                ("dy", dy, cdy),
                ("dz", dz, cdz),
                ("dw", dw, cdw),
            ] {
                assert!(
                    (a - b).abs() < 1e-2,
                    "{name} mismatch at ({x},{y},{z},{w}): analytic {a} vs CD {b}"
                );
            }
        }
    }

    #[test]
    fn fbm_wrap_xy_damped_is_periodic_in_both_axes() {
        let n = ValueNoise4D::new(7);
        let world_size = 4096.0;
        let base_freq = 1.0 / 700.0;
        for i in 0..16 {
            let t = i as f32 * 137.0;
            let a = fbm_wrap_xy_damped(&n, 0.0, t, world_size, base_freq, 6, 2.0, 0.5);
            let b = fbm_wrap_xy_damped(&n, world_size, t, world_size, base_freq, 6, 2.0, 0.5);
            assert!(
                (a - b).abs() < 1e-5,
                "damped x-wrap failed at y={t}: {a} vs {b}"
            );
            let c = fbm_wrap_xy_damped(&n, t, 0.0, world_size, base_freq, 6, 2.0, 0.5);
            let d = fbm_wrap_xy_damped(&n, t, world_size, world_size, base_freq, 6, 2.0, 0.5);
            assert!(
                (c - d).abs() < 1e-5,
                "damped y-wrap failed at x={t}: {c} vs {d}"
            );
        }
    }

    #[test]
    fn fbm_wrap_xy_damped_varies_across_both_axes() {
        let n = ValueNoise4D::new(11);
        let world_size = 4096.0;
        let base_freq = 1.0 / 700.0;
        for axis in 0..2 {
            let mut mn = f32::INFINITY;
            let mut mx = f32::NEG_INFINITY;
            for i in 0..32 {
                let t = i as f32 * (world_size / 32.0);
                let (x, y) = if axis == 0 { (t, 1000.0) } else { (1000.0, t) };
                let v = fbm_wrap_xy_damped(&n, x, y, world_size, base_freq, 6, 2.0, 0.5);
                mn = mn.min(v);
                mx = mx.max(v);
            }
            assert!(
                mx - mn > 0.05,
                "damped fBm near-constant along axis {axis}: range {}",
                mx - mn
            );
        }
    }
}
