//! Building extrusion geometry generation from footprints.
//!
//! Converts `BuildingFootprint` polygons into 3D geometry suitable for GPU
//! rendering. Each building is extruded vertically from ground (Y=0) to its
//! height. Roof is triangulated using ear-cutting. Walls are generated as
//! indexed quad strips with outward-facing normals.
//!
//! The merged vertex + index buffers allow all buildings to be rendered in
//! a single draw call. No instancing is used -- each building has unique
//! geometry with per-vertex color baked in.

use bytemuck::{Pod, Zeroable};
use velos_net::BuildingFootprint;

/// Building vertex: position + normal + color.
///
/// Layout: 40 bytes total.
/// - `position: [f32; 3]` -- world position (12 bytes)
/// - `normal: [f32; 3]` -- face normal for lighting (12 bytes)
/// - `color: [f32; 4]` -- per-vertex RGBA color (16 bytes)
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct BuildingVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 4],
}

/// Base building color: warm beige #D4C5A9 = (0.831, 0.773, 0.663).
const BASE_COLOR: [f32; 3] = [0.831, 0.773, 0.663];

/// Compute a deterministic color variation for a building based on its centroid.
///
/// Returns RGBA with the base beige color varied by +/-5% brightness,
/// using a simple hash of the centroid coordinates for determinism.
pub fn building_color_with_variation(centroid_x: f64, centroid_y: f64) -> [f32; 4] {
    // Simple deterministic hash: combine coordinate bits
    let hash_input = (centroid_x * 1000.0) as i64 ^ ((centroid_y * 1000.0) as i64).wrapping_mul(2654435761);
    // Map to [-0.05, +0.05] range
    let variation = ((hash_input.abs() % 101) as f32 / 100.0 - 0.5) * 0.10;

    [
        (BASE_COLOR[0] + variation).clamp(0.0, 1.0),
        (BASE_COLOR[1] + variation).clamp(0.0, 1.0),
        (BASE_COLOR[2] + variation).clamp(0.0, 1.0),
        1.0,
    ]
}

/// Generate merged vertex and index buffers for all building footprints.
///
/// For each building:
/// - Roof: triangulated via ear-cutting at Y = height_m, normal = [0, 1, 0]
/// - Walls: quad strips between consecutive polygon edges, with outward normals
///
/// 2D (x, y) maps to 3D (x, Y, y) per project convention.
pub fn generate_building_geometry(
    buildings: &[BuildingFootprint],
) -> (Vec<BuildingVertex>, Vec<u32>) {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    for building in buildings {
        let poly = &building.polygon;
        if poly.len() < 3 {
            continue;
        }

        let height = building.height_m as f32;

        // Compute centroid for color variation
        let n = poly.len() as f64;
        let cx: f64 = poly.iter().map(|p| p[0]).sum::<f64>() / n;
        let cy: f64 = poly.iter().map(|p| p[1]).sum::<f64>() / n;
        let color = building_color_with_variation(cx, cy);

        // --- Roof triangulation using earcutr ---
        // Flatten polygon to 2D coords for earcut
        let mut coords: Vec<f64> = Vec::with_capacity(poly.len() * 2);
        for p in poly {
            coords.push(p[0]);
            coords.push(p[1]);
        }

        let roof_indices = earcutr::earcut(&coords, &[], 2).unwrap_or_default();
        let roof_normal = [0.0_f32, 1.0, 0.0];

        let base_vertex = vertices.len() as u32;

        // Add roof vertices (at Y = height)
        for p in poly {
            vertices.push(BuildingVertex {
                position: [p[0] as f32, height, p[1] as f32],
                normal: roof_normal,
                color,
            });
        }

        // Add roof indices
        for &idx in &roof_indices {
            indices.push(base_vertex + idx as u32);
        }

        // --- Wall geometry ---
        // For CCW polygon, outward normal = cross(up, edge_direction)
        // edge_direction = (next - current) normalized
        // up = (0, 1, 0)
        // outward = cross(up, edge_dir) = (edge_dir.z, 0, -edge_dir.x)
        // In our mapping: 2D (x, y) -> 3D (x, Y, y)
        // So edge in 3D: (dx, 0, dy), up = (0, 1, 0)
        // cross(up, edge) = (dy, 0, -dx) ... but we want outward for CCW
        // Actually: cross(edge, up) = (0*0 - dy*1, dy*0 - 0*0, dx*1 - 0*0) -- nope
        // Let's be precise:
        // edge = (dx, 0, dy), up = (0, 1, 0)
        // cross(up, edge) = (1*dy - 0*0, 0*dx - 0*dy, 0*0 - 1*dx) = (dy, 0, -dx)
        // For CCW polygon with outward normal: we want the normal pointing away.
        // For a CCW polygon in the XZ plane, the outward normal of edge from A to B
        // is the right-hand perpendicular: (dy, 0, -dx) normalized.

        for i in 0..poly.len() {
            let j = (i + 1) % poly.len();
            let p0 = poly[i];
            let p1 = poly[j];

            let dx = (p1[0] - p0[0]) as f32;
            let dy = (p1[1] - p0[1]) as f32;
            let len = (dx * dx + dy * dy).sqrt();
            if len < 1e-6 {
                continue;
            }

            // Outward normal for CCW polygon in XZ plane
            let normal = [dy / len, 0.0, -dx / len];

            let wall_base = vertices.len() as u32;

            // 4 vertices per wall quad: bottom-left, bottom-right, top-right, top-left
            // Bottom-left (p0, y=0)
            vertices.push(BuildingVertex {
                position: [p0[0] as f32, 0.0, p0[1] as f32],
                normal,
                color,
            });
            // Bottom-right (p1, y=0)
            vertices.push(BuildingVertex {
                position: [p1[0] as f32, 0.0, p1[1] as f32],
                normal,
                color,
            });
            // Top-right (p1, y=height)
            vertices.push(BuildingVertex {
                position: [p1[0] as f32, height, p1[1] as f32],
                normal,
                color,
            });
            // Top-left (p0, y=height)
            vertices.push(BuildingVertex {
                position: [p0[0] as f32, height, p0[1] as f32],
                normal,
                color,
            });

            // Two triangles per quad, CCW when viewed from outside:
            // (0,2,1) and (0,3,2) — reversed so front face is outward
            indices.push(wall_base);
            indices.push(wall_base + 2);
            indices.push(wall_base + 1);

            indices.push(wall_base);
            indices.push(wall_base + 3);
            indices.push(wall_base + 2);
        }
    }

    (vertices, indices)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_building_vertex_size() {
        assert_eq!(
            std::mem::size_of::<BuildingVertex>(),
            40,
            "BuildingVertex should be 40 bytes: position(12) + normal(12) + color(16)"
        );
    }

    #[test]
    fn test_building_vertex_pod_zeroable() {
        // Verify Pod and Zeroable derive by constructing zeroed instance
        let zero: BuildingVertex = bytemuck::Zeroable::zeroed();
        assert_eq!(zero.position, [0.0, 0.0, 0.0]);
        assert_eq!(zero.normal, [0.0, 0.0, 0.0]);
        assert_eq!(zero.color, [0.0, 0.0, 0.0, 0.0]);

        // Verify Pod by casting to bytes
        let bytes: &[u8] = bytemuck::bytes_of(&zero);
        assert_eq!(bytes.len(), 40);
    }

    #[test]
    fn test_square_building_geometry() {
        // Simple 10m x 10m square at height 15m
        let building = BuildingFootprint {
            polygon: vec![
                [0.0, 0.0],
                [10.0, 0.0],
                [10.0, 10.0],
                [0.0, 10.0],
            ],
            height_m: 15.0,
        };

        let (vertices, indices) = generate_building_geometry(&[building]);

        // Roof: 4 vertices for the polygon
        // Walls: 4 walls * 4 vertices each = 16
        // Total vertices: 4 + 16 = 20
        assert_eq!(vertices.len(), 20, "Square: 4 roof + 4*4 wall = 20 vertices");

        // Roof: earcut of a square = 2 triangles = 6 indices
        // Walls: 4 walls * 2 triangles * 3 indices = 24
        // Total indices: 6 + 24 = 30
        assert_eq!(indices.len(), 30, "Square: 6 roof + 24 wall = 30 indices");
    }

    #[test]
    fn test_wall_normals_point_outward() {
        // Square building CCW: (0,0) -> (10,0) -> (10,10) -> (0,10)
        let building = BuildingFootprint {
            polygon: vec![
                [0.0, 0.0],
                [10.0, 0.0],
                [10.0, 10.0],
                [0.0, 10.0],
            ],
            height_m: 10.0,
        };

        let (vertices, _indices) = generate_building_geometry(&[building]);

        // Wall vertices start at index 4 (after 4 roof vertices)
        // Each wall has 4 vertices. Check normals for each wall.

        // Wall 0: edge (0,0)->(10,0), dx=10,dy=0 -> normal = (0, 0, -10)/10 = (0, 0, -1)
        let wall0_normal = vertices[4].normal;
        assert!(
            (wall0_normal[0]).abs() < 1e-5 && (wall0_normal[1]).abs() < 1e-5 && (wall0_normal[2] + 1.0).abs() < 1e-5,
            "Wall 0 normal should be (0, 0, -1), got {:?}", wall0_normal
        );

        // Wall 1: edge (10,0)->(10,10), dx=0,dy=10 -> normal = (10, 0, 0)/10 = (1, 0, 0)
        let wall1_normal = vertices[8].normal;
        assert!(
            (wall1_normal[0] - 1.0).abs() < 1e-5 && (wall1_normal[1]).abs() < 1e-5 && (wall1_normal[2]).abs() < 1e-5,
            "Wall 1 normal should be (1, 0, 0), got {:?}", wall1_normal
        );

        // Wall 2: edge (10,10)->(0,10), dx=-10,dy=0 -> normal = (0, 0, 10)/10 = (0, 0, 1)
        let wall2_normal = vertices[12].normal;
        assert!(
            (wall2_normal[0]).abs() < 1e-5 && (wall2_normal[1]).abs() < 1e-5 && (wall2_normal[2] - 1.0).abs() < 1e-5,
            "Wall 2 normal should be (0, 0, 1), got {:?}", wall2_normal
        );

        // Wall 3: edge (0,10)->(0,0), dx=0,dy=-10 -> normal = (-10, 0, 0)/10 = (-1, 0, 0)
        let wall3_normal = vertices[16].normal;
        assert!(
            (wall3_normal[0] + 1.0).abs() < 1e-5 && (wall3_normal[1]).abs() < 1e-5 && (wall3_normal[2]).abs() < 1e-5,
            "Wall 3 normal should be (-1, 0, 0), got {:?}", wall3_normal
        );
    }

    #[test]
    fn test_roof_normals_point_up() {
        let building = BuildingFootprint {
            polygon: vec![
                [0.0, 0.0],
                [10.0, 0.0],
                [10.0, 10.0],
                [0.0, 10.0],
            ],
            height_m: 10.0,
        };

        let (vertices, _) = generate_building_geometry(&[building]);

        // First 4 vertices are roof
        for i in 0..4 {
            assert_eq!(
                vertices[i].normal, [0.0, 1.0, 0.0],
                "Roof normal at vertex {i} should be [0, 1, 0], got {:?}",
                vertices[i].normal
            );
        }
    }

    #[test]
    fn test_roof_at_correct_height() {
        let building = BuildingFootprint {
            polygon: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
            height_m: 25.0,
        };

        let (vertices, _) = generate_building_geometry(&[building]);

        // Roof vertices (first 4) should be at Y=25.0
        for i in 0..4 {
            assert!(
                (vertices[i].position[1] - 25.0).abs() < 1e-5,
                "Roof vertex Y should be 25.0, got {}", vertices[i].position[1]
            );
        }
    }

    #[test]
    fn test_wall_base_at_ground() {
        let building = BuildingFootprint {
            polygon: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
            height_m: 20.0,
        };

        let (vertices, _) = generate_building_geometry(&[building]);

        // Wall vertices start at index 4. Each wall quad: bottom-left(y=0), bottom-right(y=0), top-right(y=h), top-left(y=h)
        for wall in 0..4 {
            let base = 4 + wall * 4;
            assert!(
                (vertices[base].position[1]).abs() < 1e-5,
                "Wall {} bottom-left Y should be 0.0", wall
            );
            assert!(
                (vertices[base + 1].position[1]).abs() < 1e-5,
                "Wall {} bottom-right Y should be 0.0", wall
            );
            assert!(
                (vertices[base + 2].position[1] - 20.0).abs() < 1e-5,
                "Wall {} top-right Y should be 20.0", wall
            );
            assert!(
                (vertices[base + 3].position[1] - 20.0).abs() < 1e-5,
                "Wall {} top-left Y should be 20.0", wall
            );
        }
    }

    #[test]
    fn test_building_color_variation_within_range() {
        // Test multiple centroid positions
        for x in [0.0, 100.0, -500.0, 12345.6] {
            for y in [0.0, 200.0, -300.0, 78901.2] {
                let color = building_color_with_variation(x, y);

                // Each RGB channel should be within +/-5% of base
                assert!(color[0] >= BASE_COLOR[0] - 0.051 && color[0] <= BASE_COLOR[0] + 0.051,
                    "R channel out of range: {}", color[0]);
                assert!(color[1] >= BASE_COLOR[1] - 0.051 && color[1] <= BASE_COLOR[1] + 0.051,
                    "G channel out of range: {}", color[1]);
                assert!(color[2] >= BASE_COLOR[2] - 0.051 && color[2] <= BASE_COLOR[2] + 0.051,
                    "B channel out of range: {}", color[2]);
                assert!((color[3] - 1.0).abs() < f32::EPSILON, "Alpha should be 1.0");
            }
        }
    }

    #[test]
    fn test_color_variation_deterministic() {
        let c1 = building_color_with_variation(100.0, 200.0);
        let c2 = building_color_with_variation(100.0, 200.0);
        assert_eq!(c1, c2, "Same centroid should produce same color");
    }

    #[test]
    fn test_empty_buildings_produces_empty_buffers() {
        let (vertices, indices) = generate_building_geometry(&[]);
        assert!(vertices.is_empty());
        assert!(indices.is_empty());
    }

    #[test]
    fn test_degenerate_polygon_skipped() {
        let building = BuildingFootprint {
            polygon: vec![[0.0, 0.0], [1.0, 0.0]], // Only 2 vertices
            height_m: 10.0,
        };
        let (vertices, indices) = generate_building_geometry(&[building]);
        assert!(vertices.is_empty());
        assert!(indices.is_empty());
    }

    #[test]
    fn test_triangle_building_geometry() {
        // Triangular building
        let building = BuildingFootprint {
            polygon: vec![
                [0.0, 0.0],
                [10.0, 0.0],
                [5.0, 8.66],
            ],
            height_m: 12.0,
        };

        let (vertices, indices) = generate_building_geometry(&[building]);

        // Roof: 3 vertices, 1 triangle = 3 indices
        // Walls: 3 walls * 4 vertices = 12, 3 walls * 6 indices = 18
        // Total: 15 vertices, 21 indices
        assert_eq!(vertices.len(), 15, "Triangle: 3 roof + 3*4 wall = 15 vertices");
        assert_eq!(indices.len(), 21, "Triangle: 3 roof + 18 wall = 21 indices");
    }

    #[test]
    fn test_multiple_buildings_merged() {
        let buildings = vec![
            BuildingFootprint {
                polygon: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
                height_m: 10.0,
            },
            BuildingFootprint {
                polygon: vec![[20.0, 0.0], [30.0, 0.0], [30.0, 10.0], [20.0, 10.0]],
                height_m: 20.0,
            },
        ];

        let (vertices, indices) = generate_building_geometry(&buildings);

        // Each square: 20 vertices, 30 indices
        assert_eq!(vertices.len(), 40, "2 squares: 2*20 = 40 vertices");
        assert_eq!(indices.len(), 60, "2 squares: 2*30 = 60 indices");

        // Second building indices should reference vertices >= 20
        let second_roof_idx = indices[30]; // First index of second building
        assert!(second_roof_idx >= 20, "Second building indices should reference offset vertices");
    }

    #[test]
    fn test_coordinate_mapping_2d_to_3d() {
        // Verify 2D (x, y) maps to 3D (x, Y, y)
        let building = BuildingFootprint {
            polygon: vec![[100.0, 200.0], [110.0, 200.0], [110.0, 210.0], [100.0, 210.0]],
            height_m: 5.0,
        };

        let (vertices, _) = generate_building_geometry(&[building]);

        // Check first roof vertex: 2D (100, 200) -> 3D (100, 5.0, 200)
        assert!((vertices[0].position[0] - 100.0).abs() < 1e-3, "X should be 100");
        assert!((vertices[0].position[1] - 5.0).abs() < 1e-3, "Y should be height=5.0");
        assert!((vertices[0].position[2] - 200.0).abs() < 1e-3, "Z should be 200 (2D y)");
    }
}
