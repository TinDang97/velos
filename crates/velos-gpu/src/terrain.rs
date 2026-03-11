//! SRTM DEM heightmap parsing and terrain mesh generation.
//!
//! Parses SRTM .hgt binary files (both SRTM1 at 1 arc-second and SRTM3 at
//! 3 arc-second resolution) into elevation grids, then generates indexed
//! triangle meshes for GPU rendering.
//!
//! Terrain vertices use the same layout as ground_plane (position vec3 + color vec4 = 28 bytes)
//! for pipeline compatibility.

use bytemuck::{Pod, Zeroable};
use std::io;
use std::path::Path;

use velos_net::EquirectangularProjection;

// --- Constants ---

/// SRTM1 (1 arc-second) samples per side: 3601.
pub const SRTM1_SAMPLES: usize = 3601;

/// SRTM3 (3 arc-second) samples per side: 1201.
pub const SRTM3_SAMPLES: usize = 1201;

/// SRTM void (no-data) sentinel value.
pub const SRTM_VOID: i16 = -32768;

/// Muted green terrain color (#3a5a3a).
pub const GROUND_COLOR: [f32; 4] = [0.227, 0.353, 0.227, 1.0];

/// Maximum terrain Y value -- clamped below road surface level.
pub const MAX_TERRAIN_Y: f32 = -0.5;

// --- Vertex type ---

/// Terrain vertex: position (vec3) + color (vec4).
/// Layout matches ground_plane.wgsl VertexInput for pipeline reuse.
/// Total size: 28 bytes (3*4 + 4*4).
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct TerrainVertex {
    pub position: [f32; 3],
    pub color: [f32; 4],
}

// --- SRTM Parsing ---

/// Parse an SRTM .hgt file into an elevation grid.
///
/// Returns `(elevations, samples_per_side)` where elevations is a row-major
/// flat array (north to south, west to east). Void values (-32768) are
/// replaced with 0 (sea level).
///
/// Supports both SRTM1 (3601x3601, ~25MB) and SRTM3 (1201x1201, ~2.8MB).
pub fn parse_hgt(path: &Path) -> Result<(Vec<i16>, usize), io::Error> {
    let data = std::fs::read(path)?;
    let samples = match data.len() {
        len if len == SRTM1_SAMPLES * SRTM1_SAMPLES * 2 => SRTM1_SAMPLES,
        len if len == SRTM3_SAMPLES * SRTM3_SAMPLES * 2 => SRTM3_SAMPLES,
        len => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "Invalid .hgt file size: {len} bytes. \
                     Expected {} (SRTM1) or {} (SRTM3)",
                    SRTM1_SAMPLES * SRTM1_SAMPLES * 2,
                    SRTM3_SAMPLES * SRTM3_SAMPLES * 2,
                ),
            ));
        }
    };

    let total = samples * samples;
    let mut elevations = Vec::with_capacity(total);
    for i in 0..total {
        let offset = i * 2;
        let raw = i16::from_be_bytes([data[offset], data[offset + 1]]);
        let value = if raw == SRTM_VOID { 0 } else { raw };
        elevations.push(value);
    }

    Ok((elevations, samples))
}

// --- Mesh Generation ---

/// Generate a terrain mesh from an elevation grid, optionally clipped to a bounding box.
///
/// `elevations`: row-major elevation data from `parse_hgt`.
/// `samples`: number of samples per side (3601 or 1201).
/// `tile_lat`: latitude of the tile's SW corner (integer degree).
/// `tile_lon`: longitude of the tile's SW corner (integer degree).
/// `proj`: equirectangular projection for lat/lon to world coordinates.
/// `bbox`: optional `(min_lat, min_lon, max_lat, max_lon)` to clip the mesh.
///
/// Returns `(vertices, indices)` for indexed triangle rendering.
pub fn generate_terrain_mesh(
    elevations: &[i16],
    samples: usize,
    tile_lat: f64,
    tile_lon: f64,
    proj: &EquirectangularProjection,
    bbox: Option<(f64, f64, f64, f64)>,
) -> (Vec<TerrainVertex>, Vec<u32>) {
    let step = 1.0 / (samples - 1) as f64;

    // Determine row/col range from bbox (or full grid).
    let (row_start, row_end, col_start, col_end) = if let Some((min_lat, min_lon, max_lat, max_lon)) = bbox {
        // Row 0 = northernmost (tile_lat + 1.0), row (samples-1) = tile_lat
        // lat = tile_lat + 1.0 - row * step => row = (tile_lat + 1.0 - lat) / step
        let row_for_lat = |lat: f64| -> usize {
            let r = ((tile_lat + 1.0 - lat) / step).floor() as isize;
            r.clamp(0, (samples - 1) as isize) as usize
        };
        let col_for_lon = |lon: f64| -> usize {
            let c = ((lon - tile_lon) / step).floor() as isize;
            c.clamp(0, (samples - 1) as isize) as usize
        };

        let rs = row_for_lat(max_lat); // max_lat is north = smaller row
        let re = row_for_lat(min_lat); // min_lat is south = larger row
        let cs = col_for_lon(min_lon);
        let ce = col_for_lon(max_lon);

        (rs, re.min(samples - 1), cs, ce.min(samples - 1))
    } else {
        (0, samples - 1, 0, samples - 1)
    };

    let num_rows = row_end - row_start + 1;
    let num_cols = col_end - col_start + 1;

    let mut vertices = Vec::with_capacity(num_rows * num_cols);

    // Build vertices
    for row in row_start..=row_end {
        for col in col_start..=col_end {
            let lat = tile_lat + 1.0 - row as f64 * step;
            let lon = tile_lon + col as f64 * step;

            let (x, y_proj) = proj.project(lat, lon);
            let elevation = elevations[row * samples + col] as f32;

            // Clamp Y to MAX_TERRAIN_Y (below road surface)
            let y = elevation.min(MAX_TERRAIN_Y);

            vertices.push(TerrainVertex {
                position: [x as f32, y, y_proj as f32],
                color: GROUND_COLOR,
            });
        }
    }

    // Build indices: two triangles per quad
    let mut indices = Vec::with_capacity((num_rows - 1) * (num_cols - 1) * 6);
    for row in 0..num_rows - 1 {
        for col in 0..num_cols - 1 {
            let tl = (row * num_cols + col) as u32;
            let tr = tl + 1;
            let bl = ((row + 1) * num_cols + col) as u32;
            let br = bl + 1;

            // Triangle 1: tl, bl, tr
            indices.push(tl);
            indices.push(bl);
            indices.push(tr);

            // Triangle 2: tr, bl, br
            indices.push(tr);
            indices.push(bl);
            indices.push(br);
        }
    }

    (vertices, indices)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_terrain_vertex_size() {
        // 3 floats (position) + 4 floats (color) = 7 * 4 = 28 bytes
        assert_eq!(
            std::mem::size_of::<TerrainVertex>(),
            28,
            "TerrainVertex should be 28 bytes"
        );
    }

    #[test]
    fn test_parse_hgt_srtm1_size() {
        // SRTM1: 3601 * 3601 * 2 = 25,934,402 bytes
        let expected = SRTM1_SAMPLES * SRTM1_SAMPLES * 2;
        assert_eq!(expected, 25_934_402);
    }

    #[test]
    fn test_parse_hgt_srtm3_size() {
        // SRTM3: 1201 * 1201 * 2 = 2,884,802 bytes
        let expected = SRTM3_SAMPLES * SRTM3_SAMPLES * 2;
        assert_eq!(expected, 2_884_802);
    }

    #[test]
    fn test_parse_hgt_rejects_invalid_size() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("invalid.hgt");
        std::fs::write(&path, &[0u8; 100]).unwrap();

        let result = parse_hgt(&path);
        assert!(result.is_err(), "Should reject invalid file size");
        let err = result.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_hgt_reads_big_endian() {
        // Construct a synthetic 3x3 SRTM3-like file (but we can't use 1201x1201 for a unit test).
        // Instead, test the big-endian parsing logic directly with a minimal SRTM3 file.
        // We'll create a proper SRTM3 file (1201*1201*2 bytes) with known values at corners.
        let samples = SRTM3_SAMPLES;
        let total = samples * samples;
        let mut data = vec![0u8; total * 2];

        // Set elevation at (0,0) = 100 (big-endian)
        let val: i16 = 100;
        let bytes = val.to_be_bytes();
        data[0] = bytes[0];
        data[1] = bytes[1];

        // Set elevation at (0,1) = 200
        let val: i16 = 200;
        let bytes = val.to_be_bytes();
        data[2] = bytes[0];
        data[3] = bytes[1];

        // Set void at (1,0) = -32768
        let val: i16 = SRTM_VOID;
        let bytes = val.to_be_bytes();
        let offset = samples * 2; // row 1, col 0
        data[offset] = bytes[0];
        data[offset + 1] = bytes[1];

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("N10E106.hgt");
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(&data).unwrap();

        let (elevations, s) = parse_hgt(&path).unwrap();
        assert_eq!(s, SRTM3_SAMPLES);
        assert_eq!(elevations[0], 100, "First value should be 100");
        assert_eq!(elevations[1], 200, "Second value should be 200");
        assert_eq!(elevations[samples], 0, "Void value should be replaced with 0");
    }

    #[test]
    fn test_void_replaced_with_zero() {
        let samples = SRTM3_SAMPLES;
        let total = samples * samples;
        let mut data = vec![0u8; total * 2];

        // Set all values to void (-32768)
        for i in 0..total {
            let bytes = SRTM_VOID.to_be_bytes();
            data[i * 2] = bytes[0];
            data[i * 2 + 1] = bytes[1];
        }

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("void.hgt");
        std::fs::write(&path, &data).unwrap();

        let (elevations, _) = parse_hgt(&path).unwrap();
        assert!(
            elevations.iter().all(|&v| v == 0),
            "All void values should be replaced with 0"
        );
    }

    #[test]
    fn test_generate_terrain_mesh_vertex_count() {
        // 3x3 grid = 9 vertices
        let elevations = vec![0i16; 9];
        let proj = EquirectangularProjection::new(10.0, 106.0);
        let (verts, _) = generate_terrain_mesh(&elevations, 3, 10.0, 106.0, &proj, None);
        assert_eq!(verts.len(), 9, "3x3 grid should produce 9 vertices");
    }

    #[test]
    fn test_generate_terrain_mesh_index_count() {
        // 3x3 grid: (3-1)*(3-1) = 4 quads, each 2 triangles * 3 indices = 24 indices
        let elevations = vec![0i16; 9];
        let proj = EquirectangularProjection::new(10.0, 106.0);
        let (_, indices) = generate_terrain_mesh(&elevations, 3, 10.0, 106.0, &proj, None);
        assert_eq!(
            indices.len(),
            24,
            "3x3 grid: (2*2)*6 = 24 indices"
        );
    }

    #[test]
    fn test_terrain_y_clamped_below_road() {
        // Elevation of 15m should still be clamped to MAX_TERRAIN_Y
        let elevations = vec![15i16; 4]; // 2x2 grid
        let proj = EquirectangularProjection::new(10.0, 106.0);
        let (verts, _) = generate_terrain_mesh(&elevations, 2, 10.0, 106.0, &proj, None);
        for v in &verts {
            assert!(
                v.position[1] <= MAX_TERRAIN_Y,
                "Terrain Y should be <= {MAX_TERRAIN_Y}, got {}",
                v.position[1]
            );
        }
    }

    #[test]
    fn test_terrain_color_is_muted_green() {
        let elevations = vec![0i16; 4];
        let proj = EquirectangularProjection::new(10.0, 106.0);
        let (verts, _) = generate_terrain_mesh(&elevations, 2, 10.0, 106.0, &proj, None);
        for v in &verts {
            assert!(
                (v.color[0] - 0.227).abs() < 0.001
                    && (v.color[1] - 0.353).abs() < 0.001
                    && (v.color[2] - 0.227).abs() < 0.001
                    && (v.color[3] - 1.0).abs() < 0.001,
                "Terrain color should be muted green #3a5a3a, got {:?}",
                v.color
            );
        }
    }

    #[test]
    fn test_generate_terrain_mesh_with_bbox() {
        // 5x5 grid, clip to center 3x3
        let elevations = vec![0i16; 25];
        let proj = EquirectangularProjection::new(10.0, 106.0);
        // Tile covers lat 10-11, lon 106-107. Clip to center region.
        let bbox = Some((10.3, 106.3, 10.7, 106.7));
        let (verts, indices) = generate_terrain_mesh(&elevations, 5, 10.0, 106.0, &proj, bbox);

        // With 5 samples, step = 0.25 deg.
        // Rows for lat: row=0 -> lat=11.0, row=1 -> 10.75, row=2 -> 10.5, row=3 -> 10.25, row=4 -> 10.0
        // bbox max_lat=10.7 -> row = floor((11.0-10.7)/0.25) = floor(1.2) = 1
        // bbox min_lat=10.3 -> row = floor((11.0-10.3)/0.25) = floor(2.8) = 2
        // Cols for lon: col = floor((lon-106.0)/0.25)
        // bbox min_lon=106.3 -> col = floor(1.2) = 1
        // bbox max_lon=106.7 -> col = floor(2.8) = 2
        // So rows 1..2 (2 rows), cols 1..2 (2 cols) = 4 vertices
        assert_eq!(verts.len(), 4, "Clipped 5x5 to center bbox should produce 4 vertices");
        assert_eq!(indices.len(), 6, "2x2 clipped region: 1 quad = 6 indices");
    }

    #[test]
    fn test_terrain_4x4_index_count() {
        // 4x4 grid: (3)*(3) = 9 quads * 6 = 54 indices
        let elevations = vec![0i16; 16];
        let proj = EquirectangularProjection::new(10.0, 106.0);
        let (verts, indices) = generate_terrain_mesh(&elevations, 4, 10.0, 106.0, &proj, None);
        assert_eq!(verts.len(), 16);
        assert_eq!(indices.len(), 54, "4x4 grid: 3*3*6 = 54 indices");
    }
}
