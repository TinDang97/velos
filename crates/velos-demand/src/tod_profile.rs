//! Time-of-day demand scaling profiles with piecewise-linear interpolation.

/// A time-of-day profile defined by (hour, factor) control points.
///
/// Interpolates linearly between points. Before the first point, uses first
/// point's factor. After the last point, uses last point's factor.
#[derive(Debug, Clone)]
pub struct TodProfile {
    /// Sorted (hour, factor) control points. Must have at least one point.
    points: Vec<(f64, f64)>,
}

impl TodProfile {
    /// Create a profile from (hour, factor) control points.
    ///
    /// Points are sorted by hour on construction.
    ///
    /// # Panics
    /// Panics if `points` is empty.
    pub fn new(mut points: Vec<(f64, f64)>) -> Self {
        assert!(!points.is_empty(), "TodProfile requires at least one point");
        points.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        Self { points }
    }

    /// Factory: HCMC weekday demand profile with AM/PM peaks.
    ///
    /// Based on HCMC traffic survey data:
    /// - AM peak: 07:00-08:00 (factor 1.0)
    /// - PM peak: 17:00-18:00 (factor 1.0)
    /// - Midday plateau: 12:00 (factor 0.7)
    /// - Late night trough: 00:00-05:00 (factor 0.05-0.10)
    pub fn hcmc_weekday() -> Self {
        Self::new(vec![
            (0.0, 0.05),
            (5.0, 0.10),
            (6.0, 0.40),
            (7.0, 1.00),
            (8.0, 1.00),
            (9.0, 0.50),
            (12.0, 0.70),
            (13.0, 0.50),
            (17.0, 1.00),
            (18.0, 1.00),
            (19.0, 0.50),
            (22.0, 0.10),
        ])
    }

    /// Get the demand scaling factor at the given hour via linear interpolation.
    ///
    /// - Before the first control point: returns first point's factor.
    /// - After the last control point: returns last point's factor.
    /// - Between two points: linearly interpolates.
    /// - Exactly on a point: returns that point's factor.
    pub fn factor_at(&self, hour: f64) -> f64 {
        let first = self.points.first().unwrap();
        let last = self.points.last().unwrap();

        if hour <= first.0 {
            return first.1;
        }
        if hour >= last.0 {
            return last.1;
        }

        // Find the bracketing points via binary search.
        // We want the rightmost point with hour <= target.
        let idx = self
            .points
            .partition_point(|&(h, _)| h <= hour)
            .saturating_sub(1);

        let (h0, f0) = self.points[idx];
        let (h1, f1) = self.points[idx + 1];

        let t = (hour - h0) / (h1 - h0);
        f0 + t * (f1 - f0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_point_returns_exact_factor() {
        let tod = TodProfile::new(vec![(0.0, 0.0), (5.0, 0.5), (10.0, 1.0)]);
        assert!((tod.factor_at(5.0) - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn quarter_interpolation() {
        let tod = TodProfile::new(vec![(0.0, 0.0), (10.0, 1.0)]);
        assert!((tod.factor_at(2.5) - 0.25).abs() < 0.001);
    }

    #[test]
    #[should_panic(expected = "at least one point")]
    fn empty_points_panics() {
        TodProfile::new(vec![]);
    }
}
