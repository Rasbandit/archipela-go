//! Small fixed-size matrices for the filters (2x2 and 4x4 are all they need), so no linear-algebra crate is added.

use crate::num::count_f64;

/// A matrix of `R` rows and `C` columns, row-major.
pub type Mat<const R: usize, const C: usize> = [[f64; C]; R];

/// The zero matrix.
#[must_use]
pub fn zeros<const R: usize, const C: usize>() -> Mat<R, C> {
    [[0.0; C]; R]
}

/// The identity.
#[must_use]
pub fn identity<const N: usize>() -> Mat<N, N> {
    std::array::from_fn(|i| std::array::from_fn(|j| if i == j { 1.0 } else { 0.0 }))
}

/// `a * b`.
#[must_use]
pub fn mul<const R: usize, const K: usize, const C: usize>(a: &Mat<R, K>, b: &Mat<K, C>) -> Mat<R, C> {
    std::array::from_fn(|i| std::array::from_fn(|j| (0..K).map(|k| a[i][k] * b[k][j]).sum()))
}

/// `a * v`.
#[must_use]
pub fn mul_vec<const R: usize, const C: usize>(a: &Mat<R, C>, v: &[f64; C]) -> [f64; R] {
    std::array::from_fn(|i| a[i].iter().zip(v).map(|(x, y)| x * y).sum())
}

/// The transpose.
#[must_use]
pub fn transpose<const R: usize, const C: usize>(a: &Mat<R, C>) -> Mat<C, R> {
    std::array::from_fn(|j| std::array::from_fn(|i| a[i][j]))
}

/// `a + b`.
#[must_use]
pub fn add<const R: usize, const C: usize>(a: &Mat<R, C>, b: &Mat<R, C>) -> Mat<R, C> {
    std::array::from_fn(|i| std::array::from_fn(|j| a[i][j] + b[i][j]))
}

/// `a - b`.
#[must_use]
pub fn sub<const R: usize, const C: usize>(a: &Mat<R, C>, b: &Mat<R, C>) -> Mat<R, C> {
    std::array::from_fn(|i| std::array::from_fn(|j| a[i][j] - b[i][j]))
}

/// `s * a`.
#[must_use]
pub fn scale<const R: usize, const C: usize>(a: &Mat<R, C>, s: f64) -> Mat<R, C> {
    std::array::from_fn(|i| std::array::from_fn(|j| a[i][j] * s))
}

/// `(a + a^T) / 2`: keeps a covariance exactly symmetric under rounding.
#[must_use]
pub fn symmetrize<const N: usize>(a: &Mat<N, N>) -> Mat<N, N> {
    std::array::from_fn(|i| std::array::from_fn(|j| f64::midpoint(a[i][j], a[j][i])))
}

/// `a b^T`.
#[must_use]
pub fn outer<const N: usize>(a: &[f64; N], b: &[f64; N]) -> Mat<N, N> {
    std::array::from_fn(|i| std::array::from_fn(|j| a[i] * b[j]))
}

/// `a . b`.
#[must_use]
pub fn dot<const N: usize>(a: &[f64; N], b: &[f64; N]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// The inverse and the determinant of `a` (Gauss-Jordan with partial pivoting); `None` when it is singular.
#[must_use]
pub fn inverse<const N: usize>(a: &Mat<N, N>) -> Option<(Mat<N, N>, f64)> {
    let (mut m, mut inv, mut det) = (*a, identity::<N>(), 1.0);
    for col in 0..N {
        let pivot = (col..N).max_by(|&x, &y| m[x][col].abs().total_cmp(&m[y][col].abs()))?;
        if m[pivot][col].abs() < 1e-12 * (1.0 + a[col][col].abs()) {
            return None;
        }
        if pivot != col {
            m.swap(pivot, col);
            inv.swap(pivot, col);
            det = -det;
        }
        let d = m[col][col];
        det *= d;
        m[col] = m[col].map(|v| v / d);
        inv[col] = inv[col].map(|v| v / d);
        let (prow, irow) = (m[col], inv[col]);
        for r in (0..N).filter(|&r| r != col) {
            let f = m[r][col];
            m[r] = std::array::from_fn(|j| m[r][j] - f * prow[j]);
            inv[r] = std::array::from_fn(|j| inv[r][j] - f * irow[j]);
        }
    }
    Some((inv, det))
}

/// The largest eigenvalue of a symmetric 2x2 matrix.
#[must_use]
pub fn max_eig2(a: &Mat<2, 2>) -> f64 {
    let (m, d) = (f64::midpoint(a[0][0], a[1][1]), 0.5 * (a[0][0] - a[1][1]));
    m + d.hypot(a[0][1])
}

/// One Kalman measurement update.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Update {
    /// Posterior state.
    pub x: [f64; 4],
    /// Posterior covariance (Joseph form).
    pub p: Mat<4, 4>,
    /// Squared Mahalanobis distance of the innovation.
    pub d2: f64,
    /// Log likelihood of the measurement, `ln N(y; 0, S)`.
    pub log_likelihood: f64,
}

/// `[[a, 0], [0, b]]` of two 2x2 blocks.
#[must_use]
pub fn block_diag(a: &Mat<2, 2>, b: &Mat<2, 2>) -> Mat<4, 4> {
    std::array::from_fn(|i| {
        std::array::from_fn(|j| match (i < 2, j < 2) {
            (true, true) => a[i][j],
            (false, false) => b[i - 2][j - 2],
            _ => 0.0,
        })
    })
}

#[allow(clippy::many_single_char_names)] // x, p, z, h, r, y, s are the textbook Kalman symbols
fn innovation<const M: usize>(x: &[f64; 4], p: &Mat<4, 4>, z: &[f64; M], h: &Mat<M, 4>, r: &Mat<M, M>) -> Option<([f64; M], Mat<M, M>, f64)> {
    let hx = mul_vec(h, x);
    let y: [f64; M] = std::array::from_fn(|i| z[i] - hx[i]);
    let s = add(&mul(&mul(h, p), &transpose(h)), r);
    let (s_inv, det) = inverse(&s)?;
    (det > 0.0).then_some((y, s_inv, det))
}

/// `y^T S^-1 y` of measuring `z = H x + v`, `v ~ N(0, R)` against the state; `None` when `S` is singular.
#[must_use]
#[allow(clippy::many_single_char_names)] // x, p, z, h, r, y, s are the textbook Kalman symbols
pub fn mahalanobis2<const M: usize>(x: &[f64; 4], p: &Mat<4, 4>, z: &[f64; M], h: &Mat<M, 4>, r: &Mat<M, M>) -> Option<f64> {
    let (y, s_inv, _) = innovation(x, p, z, h, r)?;
    Some(dot(&y, &mul_vec(&s_inv, &y)))
}

/// Kalman update of `x`, `p` with `z = H x + v`, `v ~ N(0, R)`. The covariance uses the Joseph form `(I-KH) P (I-KH)^T + K R K^T`, which
/// stays symmetric and positive under rounding. `None` when the innovation covariance is singular.
#[must_use]
#[allow(clippy::many_single_char_names)] // x, p, z, h, r, y, s are the textbook Kalman symbols
pub fn kalman_update<const M: usize>(x: &[f64; 4], p: &Mat<4, 4>, z: &[f64; M], h: &Mat<M, 4>, r: &Mat<M, M>) -> Option<Update> {
    let (y, s_inv, det) = innovation(x, p, z, h, r)?;
    let k = mul(&mul(p, &transpose(h)), &s_inv);
    let ky = mul_vec(&k, &y);
    let i_kh = sub(&identity::<4>(), &mul(&k, h));
    let p2 = add(&mul(&mul(&i_kh, p), &transpose(&i_kh)), &mul(&mul(&k, r), &transpose(&k)));
    let d2 = dot(&y, &mul_vec(&s_inv, &y));
    Some(Update {
        x: std::array::from_fn(|i| x[i] + ky[i]),
        p: symmetrize(&p2),
        d2,
        log_likelihood: -0.5 * (d2 + det.ln() + count_f64(M) * std::f64::consts::TAU.ln()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close<const R: usize, const C: usize>(a: &Mat<R, C>, b: &Mat<R, C>) -> bool {
        a.iter().zip(b).all(|(x, y)| x.iter().zip(y).all(|(u, v)| (u - v).abs() < 1e-9))
    }

    #[test]
    fn inverse_times_matrix_is_identity_and_the_determinant_is_right() {
        let a: Mat<4, 4> = [[4.0, 1.0, 0.0, 0.5], [1.0, 3.0, 0.2, 0.0], [0.0, 0.2, 2.0, 0.1], [0.5, 0.0, 0.1, 1.0]];
        let (inv, det) = inverse(&a).unwrap();
        assert!(close(&mul(&a, &inv), &identity::<4>()));
        let (_, d2) = inverse(&[[2.0, 1.0], [1.0, 3.0]]).unwrap();
        assert!((d2 - 5.0).abs() < 1e-12);
        assert!(det > 0.0);
        assert!(inverse(&[[1.0, 2.0], [2.0, 4.0]]).is_none(), "singular");
    }

    #[test]
    fn products_transposes_and_eigenvalues() {
        let a: Mat<2, 3> = [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]];
        assert_eq!(transpose(&a), [[1.0, 4.0], [2.0, 5.0], [3.0, 6.0]]);
        assert_eq!(mul(&a, &transpose(&a)), [[14.0, 32.0], [32.0, 77.0]]);
        assert_eq!(mul_vec(&a, &[1.0, 0.0, 1.0]), [4.0, 10.0]);
        assert!((max_eig2(&[[4.0, 0.0], [0.0, 9.0]]) - 9.0).abs() < 1e-12);
        assert!((max_eig2(&[[2.0, 1.0], [1.0, 2.0]]) - 3.0).abs() < 1e-12);
    }

    #[test]
    fn a_one_axis_update_matches_the_scalar_kalman_formula() {
        // prior x = 0 var 4, measurement 2 var 4: posterior 1, var 2; Joseph form keeps the covariance symmetric.
        let mut p = identity::<4>();
        p[0][0] = 4.0;
        let h: Mat<1, 4> = [[1.0, 0.0, 0.0, 0.0]];
        let u = kalman_update(&[0.0; 4], &p, &[2.0], &h, &[[4.0]]).unwrap();
        assert!((u.x[0] - 1.0).abs() < 1e-12 && (u.p[0][0] - 2.0).abs() < 1e-12);
        assert!((u.d2 - 0.5).abs() < 1e-12, "y^2 / S = 4 / 8");
        assert_eq!(u.p, symmetrize(&u.p));
        assert!((mahalanobis2(&[0.0; 4], &p, &[2.0], &h, &[[4.0]]).unwrap() - 0.5).abs() < 1e-12);
    }
}
