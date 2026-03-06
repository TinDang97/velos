//! Tests for the equirectangular projection module.

use velos_net::EquirectangularProjection;

const CENTER_LAT: f64 = 10.7756;
const CENTER_LON: f64 = 106.7019;

#[test]
fn center_projects_to_origin() {
    let proj = EquirectangularProjection::new(CENTER_LAT, CENTER_LON);
    let (x, y) = proj.project(CENTER_LAT, CENTER_LON);
    assert!(x.abs() < 1e-10);
    assert!(y.abs() < 1e-10);
}

#[test]
fn offset_projects_to_metres() {
    let proj = EquirectangularProjection::new(CENTER_LAT, CENTER_LON);
    // 10.7766, 106.7029 is 0.001 deg N and 0.001 deg E of center
    let (x, y) = proj.project(10.7766, 106.7029);
    // y = 0.001 * 110540 = ~110.54 m
    assert!((y - 110.54).abs() < 1.0, "y ~110.54m, got {y}");
    // x = 0.001 * cos(10.7756 rad) * 111320 ~= 109.38 m
    assert!((x - 109.38).abs() < 1.0, "x ~109.38m, got {x}");
}

#[test]
fn symmetry() {
    let proj = EquirectangularProjection::new(CENTER_LAT, CENTER_LON);
    let (x1, y1) = proj.project(CENTER_LAT + 0.01, CENTER_LON + 0.01);
    let (x2, y2) = proj.project(CENTER_LAT - 0.01, CENTER_LON - 0.01);
    assert!((x1 + x2).abs() < 1e-6, "x should be symmetric");
    assert!((y1 + y2).abs() < 1e-6, "y should be symmetric");
}

#[test]
fn roundtrip() {
    let proj = EquirectangularProjection::new(CENTER_LAT, CENTER_LON);
    let lat = 10.780;
    let lon = 106.705;
    let (x, y) = proj.project(lat, lon);
    let (lat2, lon2) = proj.unproject(x, y);
    assert!((lat2 - lat).abs() < 1e-9);
    assert!((lon2 - lon).abs() < 1e-9);
}
