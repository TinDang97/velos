//! Headless render pipeline tests for REN-01 and REN-02.
//! These tests create pipelines without a real surface using a compatible texture format.
//! Gated by `--features gpu-tests`.

#![cfg(feature = "gpu-tests")]

use velos_gpu::{Camera2D, GpuContext, Renderer};

fn skip_if_no_gpu() -> Option<GpuContext> {
    GpuContext::new_headless()
}

/// REN-01: Device and render pipeline initialize without panic.
#[test]
fn test_render_pipeline_creation() {
    let ctx = match skip_if_no_gpu() {
        Some(c) => c,
        None => {
            eprintln!("SKIP: no GPU adapter");
            return;
        }
    };

    // Use Bgra8UnormSrgb as a common surface format for headless testing
    let surface_format = wgpu::TextureFormat::Bgra8UnormSrgb;
    let _renderer = Renderer::new(&ctx.device, surface_format);
    // If we get here without panic, REN-01 is satisfied
}

/// REN-02: Instance buffer can be populated with 1K agent instances.
#[test]
fn test_instanced_render() {
    let ctx = match skip_if_no_gpu() {
        Some(c) => c,
        None => {
            eprintln!("SKIP: no GPU adapter");
            return;
        }
    };

    let surface_format = wgpu::TextureFormat::Bgra8UnormSrgb;
    let mut renderer = Renderer::new(&ctx.device, surface_format);

    // Build 1K fake positions and headings
    let positions: Vec<[f32; 2]> =
        (0..1000).map(|i| [i as f32, (i as f32) * 0.5]).collect();
    let headings: Vec<f32> = (0..1000).map(|i| (i as f32) * 0.01).collect();

    // Should not panic
    renderer.update_instances_from_cpu(&ctx.queue, &positions, &headings);
    let _ = ctx.device.poll(wgpu::PollType::wait_indefinitely());
}

/// REN-03: Camera projection test (no GPU needed -- pure CPU math).
#[test]
fn test_camera_projection_headless() {
    use glam::Vec2;

    let cam = Camera2D::new(Vec2::new(1280.0, 720.0));
    let m = cam.view_proj_matrix();
    let ndc = m * glam::Vec4::new(0.0, 0.0, 0.0, 1.0);
    assert!(ndc.x.abs() < 1e-5, "World origin should map to NDC x=0");
    assert!(ndc.y.abs() < 1e-5, "World origin should map to NDC y=0");
}
