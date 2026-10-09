//! Small, saturating numeric conversions for counts that are far below the target type's limits.

/// A count as `u32`, saturating at `u32::MAX` (counts here are bounded by game size, so saturation is unreachable in practice).
#[must_use]
pub fn count_u32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// A count as `i64`, saturating at `i64::MAX`.
pub(crate) fn count_i64(n: usize) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}

/// A count as `f64`; exact for any count below 2^52.
pub(crate) fn count_f64(n: usize) -> f64 {
    #[allow(clippy::cast_precision_loss)] // counts here are far below 2^52, where f64 is exact
    {
        n as f64
    }
}

/// A count as `f32`, for progress ratios where rounding beyond 2^24 does not matter.
pub(crate) fn count_f32(n: usize) -> f32 {
    #[allow(clippy::cast_precision_loss)] // only used for progress ratios, where rounding is invisible
    {
        n as f32
    }
}

/// An `f64` narrowed to `f32` for display and progress values.
pub(crate) fn to_f32(x: f64) -> f32 {
    #[allow(clippy::cast_possible_truncation)] // progress and display values do not need f64 precision
    {
        x as f32
    }
}

/// `x` rounded down to a whole number. Saturates at the `i64` limits; NaN gives 0.
pub(crate) fn floor_i64(x: f64) -> i64 {
    #[allow(clippy::cast_possible_truncation)] // float-to-int `as` saturates; map coordinates are far inside the range
    {
        x.floor() as i64
    }
}

/// `x` rounded to the nearest whole number. Saturates at the `i64` limits; NaN gives 0.
pub(crate) fn round_i64(x: f64) -> i64 {
    #[allow(clippy::cast_possible_truncation)] // float-to-int `as` saturates; callers pass bounded map or time values
    {
        x.round() as i64
    }
}

/// `x` rounded down to a whole number, as `i32`. Saturates; NaN gives 0.
pub(crate) fn floor_i32(x: f64) -> i32 {
    #[allow(clippy::cast_possible_truncation)] // float-to-int `as` saturates; latitudes and longitudes are tiny
    {
        x.floor() as i32
    }
}

/// `x` rounded to the nearest whole number, as `u32`. Negative values give 0, huge values saturate, NaN gives 0.
pub(crate) fn round_u32(x: f64) -> u32 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // float-to-int `as` saturates and clamps negatives to 0
    {
        x.round() as u32
    }
}

/// `x` rounded to the nearest whole number, as `u64`. Negative values give 0, huge values saturate, NaN gives 0.
pub(crate) fn round_u64(x: f64) -> u64 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // float-to-int `as` saturates and clamps negatives to 0
    {
        x.round() as u64
    }
}

/// `x` rounded up to a whole number, as `usize`. Negative values give 0, huge values saturate, NaN gives 0.
pub(crate) fn ceil_usize(x: f64) -> usize {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // float-to-int `as` saturates and clamps negatives to 0
    {
        x.ceil() as usize
    }
}

/// A millisecond timestamp or duration as `f64`; exact below 2^52 (about 142,000 years).
pub(crate) fn i64_to_f64(n: i64) -> f64 {
    #[allow(clippy::cast_precision_loss)] // timestamps, durations and tile indexes here are far below 2^52
    {
        n as f64
    }
}

/// `x` with its fraction cut off, as `i64`. Saturates at the `i64` limits; NaN gives 0.
pub(crate) fn trunc_i64(x: f64) -> i64 {
    #[allow(clippy::cast_possible_truncation)] // float-to-int `as` saturates; callers pass bounded durations
    {
        x as i64
    }
}

/// `x` (finite, non-negative, already floored) as an index.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // callers pass floored, non-negative ranks
pub(crate) fn floor_usize(x: f64) -> usize {
    x.floor().max(0.0) as usize
}
