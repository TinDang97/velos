//! Time-of-day demand scaling profiles with piecewise-linear interpolation.

/// A time-of-day profile defined by (hour, factor) control points.
///
/// Interpolates linearly between points. Before the first point, uses first
/// point's factor. After the last point, uses last point's factor.
#[derive(Debug, Clone)]
pub struct TodProfile {
    points: Vec<(f64, f64)>,
}

impl TodProfile {
    /// Create a profile from sorted (hour, factor) control points.
    pub fn new(_points: Vec<(f64, f64)>) -> Self {
        todo!("implement new")
    }

    /// Factory: HCMC weekday demand profile with AM/PM peaks.
    pub fn hcmc_weekday() -> Self {
        todo!("implement hcmc_weekday")
    }

    /// Get the demand scaling factor at the given hour via linear interpolation.
    pub fn factor_at(&self, _hour: f64) -> f64 {
        todo!("implement factor_at")
    }
}
