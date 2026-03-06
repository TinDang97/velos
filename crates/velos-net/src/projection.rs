//! Equirectangular projection for converting WGS84 lat/lon to local metres.
//!
//! Centered on a reference point (typically District 1 centroid: 10.7756, 106.7019).
//! Accurate within ~0.3% for areas < 20 km across at HCMC latitude (~10.8 deg N).

/// Metres per degree of latitude (WGS84 approximation).
const DEG_TO_M_LAT: f64 = 110_540.0;

/// Metres per degree of longitude at the equator (WGS84 approximation).
const DEG_TO_M_LON_EQUATOR: f64 = 111_320.0;

/// Equirectangular projection centered on a reference point.
///
/// Projects WGS84 (lat, lon) to local (x_east, y_north) in metres.
#[derive(Debug, Clone, Copy)]
pub struct EquirectangularProjection {
    center_lat: f64,
    center_lon: f64,
    cos_center_lat: f64,
}

impl EquirectangularProjection {
    /// Create a new projection centered on the given WGS84 coordinate.
    pub fn new(center_lat: f64, center_lon: f64) -> Self {
        Self {
            center_lat,
            center_lon,
            cos_center_lat: center_lat.to_radians().cos(),
        }
    }

    /// Project a WGS84 (lat, lon) to local (x_east, y_north) in metres.
    ///
    /// The center point projects to (0.0, 0.0).
    pub fn project(&self, lat: f64, lon: f64) -> (f64, f64) {
        let x = (lon - self.center_lon) * self.cos_center_lat * DEG_TO_M_LON_EQUATOR;
        let y = (lat - self.center_lat) * DEG_TO_M_LAT;
        (x, y)
    }

    /// Inverse projection: local metres (x, y) back to WGS84 (lat, lon).
    pub fn unproject(&self, x: f64, y: f64) -> (f64, f64) {
        let lon = x / (self.cos_center_lat * DEG_TO_M_LON_EQUATOR) + self.center_lon;
        let lat = y / DEG_TO_M_LAT + self.center_lat;
        (lat, lon)
    }

    /// Returns the center latitude.
    pub fn center_lat(&self) -> f64 {
        self.center_lat
    }

    /// Returns the center longitude.
    pub fn center_lon(&self) -> f64 {
        self.center_lon
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CENTER_LAT: f64 = 10.7756;
    const CENTER_LON: f64 = 106.7019;

    #[test]
    fn center_projects_to_origin() {
        let proj = EquirectangularProjection::new(CENTER_LAT, CENTER_LON);
        let (x, y) = proj.project(CENTER_LAT, CENTER_LON);
        assert!((x).abs() < 1e-10, "x should be ~0, got {x}");
        assert!((y).abs() < 1e-10, "y should be ~0, got {y}");
    }

    #[test]
    fn offset_point_projects_correctly() {
        let proj = EquirectangularProjection::new(CENTER_LAT, CENTER_LON);
        // 0.001 degree lat north = ~110.54 m
        // 0.001 degree lon east = ~111320 * cos(10.7756 deg) ~= 109.38 m
        let (x, y) = proj.project(CENTER_LAT + 0.001, CENTER_LON + 0.001);
        assert!((y - 110.54).abs() < 1.0, "y offset ~110.54m, got {y}");
        assert!((x - 109.38).abs() < 1.0, "x offset ~109.38m, got {x}");
    }

    #[test]
    fn roundtrip_projection() {
        let proj = EquirectangularProjection::new(CENTER_LAT, CENTER_LON);
        let lat = 10.780;
        let lon = 106.705;
        let (x, y) = proj.project(lat, lon);
        let (lat2, lon2) = proj.unproject(x, y);
        assert!((lat2 - lat).abs() < 1e-9, "lat roundtrip failed");
        assert!((lon2 - lon).abs() < 1e-9, "lon roundtrip failed");
    }

    #[test]
    fn symmetry_test() {
        let proj = EquirectangularProjection::new(CENTER_LAT, CENTER_LON);
        let (x1, y1) = proj.project(CENTER_LAT + 0.01, CENTER_LON + 0.01);
        let (x2, y2) = proj.project(CENTER_LAT - 0.01, CENTER_LON - 0.01);
        assert!((x1 + x2).abs() < 1e-6, "x should be symmetric");
        assert!((y1 + y2).abs() < 1e-6, "y should be symmetric");
    }
}
