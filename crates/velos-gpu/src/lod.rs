//! LOD (Level of Detail) classification with hysteresis.
//!
//! Classifies agents into three rendering tiers based on distance to camera:
//! - **Mesh** (< 50m): Full 3D mesh with lighting
//! - **Billboard** (50-200m): Camera-facing colored quad
//! - **Dot** (> 200m): Colored dot (reuses 2D pipeline)
//!
//! Hysteresis prevents flickering at tier boundaries by requiring agents
//! to exceed `threshold * 1.1` before downgrading.

use crate::orbit_camera::{HYSTERESIS_FACTOR, LOD_BILLBOARD_THRESHOLD, LOD_MESH_THRESHOLD};

/// LOD rendering tier for an agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LodTier {
    /// Full 3D mesh rendering (nearest agents).
    Mesh,
    /// Camera-facing billboard sprite (mid-range).
    Billboard,
    /// Simple colored dot (far range, cheapest).
    Dot,
}

/// Classify an agent's LOD tier based on camera distance.
///
/// When `previous_tier` is `Some`, hysteresis is applied: downgrade only
/// happens at `threshold * HYSTERESIS_FACTOR` to prevent flickering.
/// Upgrade (getting closer) happens at the exact threshold.
pub fn classify_lod(distance: f32, previous_tier: Option<LodTier>) -> LodTier {
    match previous_tier {
        None => {
            // First frame: classify purely by distance
            if distance < LOD_MESH_THRESHOLD {
                LodTier::Mesh
            } else if distance < LOD_BILLBOARD_THRESHOLD {
                LodTier::Billboard
            } else {
                LodTier::Dot
            }
        }
        Some(prev) => {
            // Hysteresis: downgrade only at threshold * HYSTERESIS_FACTOR,
            // upgrade at exact threshold.
            match prev {
                LodTier::Mesh => {
                    if distance > LOD_MESH_THRESHOLD * HYSTERESIS_FACTOR {
                        // Downgrade from Mesh
                        if distance < LOD_BILLBOARD_THRESHOLD {
                            LodTier::Billboard
                        } else {
                            LodTier::Dot
                        }
                    } else {
                        LodTier::Mesh
                    }
                }
                LodTier::Billboard => {
                    if distance < LOD_MESH_THRESHOLD {
                        // Upgrade to Mesh
                        LodTier::Mesh
                    } else if distance > LOD_BILLBOARD_THRESHOLD * HYSTERESIS_FACTOR {
                        // Downgrade to Dot
                        LodTier::Dot
                    } else {
                        LodTier::Billboard
                    }
                }
                LodTier::Dot => {
                    if distance < LOD_MESH_THRESHOLD {
                        LodTier::Mesh
                    } else if distance < LOD_BILLBOARD_THRESHOLD {
                        LodTier::Billboard
                    } else {
                        LodTier::Dot
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_close_range_mesh() {
        assert_eq!(classify_lod(30.0, None), LodTier::Mesh);
    }

    #[test]
    fn test_classify_mid_range_billboard() {
        assert_eq!(classify_lod(100.0, None), LodTier::Billboard);
    }

    #[test]
    fn test_classify_far_range_dot() {
        assert_eq!(classify_lod(300.0, None), LodTier::Dot);
    }

    #[test]
    fn test_hysteresis_mesh_stays_at_52m() {
        // Agent at Mesh tier, distance increases to 52m
        // Downgrade threshold = 50 * 1.1 = 55m, so should stay Mesh
        assert_eq!(
            classify_lod(52.0, Some(LodTier::Mesh)),
            LodTier::Mesh,
            "Mesh should not downgrade at 52m (threshold=55m)"
        );
    }

    #[test]
    fn test_hysteresis_mesh_downgrades_at_56m() {
        // Agent at Mesh tier, distance 56m > 55m threshold
        assert_eq!(
            classify_lod(56.0, Some(LodTier::Mesh)),
            LodTier::Billboard,
            "Mesh should downgrade to Billboard at 56m (threshold=55m)"
        );
    }

    #[test]
    fn test_hysteresis_billboard_stays_at_210m() {
        // Billboard tier, distance 210m < 220m downgrade threshold
        assert_eq!(
            classify_lod(210.0, Some(LodTier::Billboard)),
            LodTier::Billboard,
            "Billboard should not downgrade at 210m (threshold=220m)"
        );
    }

    #[test]
    fn test_hysteresis_billboard_downgrades_at_225m() {
        assert_eq!(
            classify_lod(225.0, Some(LodTier::Billboard)),
            LodTier::Dot,
            "Billboard should downgrade to Dot at 225m (threshold=220m)"
        );
    }

    #[test]
    fn test_upgrade_billboard_to_mesh() {
        // Agent at Billboard tier, distance drops below 50m -> upgrade to Mesh
        assert_eq!(
            classify_lod(45.0, Some(LodTier::Billboard)),
            LodTier::Mesh,
            "Billboard should upgrade to Mesh at 45m"
        );
    }

    #[test]
    fn test_upgrade_dot_to_billboard() {
        // Agent at Dot tier, distance drops below 200m -> upgrade to Billboard
        assert_eq!(
            classify_lod(190.0, Some(LodTier::Dot)),
            LodTier::Billboard,
            "Dot should upgrade to Billboard at 190m"
        );
    }

    #[test]
    fn test_boundary_exact_mesh_threshold() {
        // At exactly 50m without previous -> Billboard (not strictly < 50)
        assert_eq!(classify_lod(50.0, None), LodTier::Billboard);
    }
}
