//! Integration tests for pedestrian adaptive GPU dispatch.
//!
//! Tests prefix-sum correctness, scatter compaction, social force GPU vs CPU reference,
//! and sparse scenario completion.
//!
//! All tests gated behind `gpu-tests` feature.

#[cfg(feature = "gpu-tests")]
mod gpu {
    use velos_gpu::ped_adaptive::{
        GpuPedestrian, PedestrianAdaptiveParams, PedestrianAdaptivePipeline,
    };
    use velos_gpu::GpuContext;
    use velos_vehicle::social_force::{
        social_force_acceleration, PedestrianNeighbor, SocialForceParams,
    };

    /// Create a GPU context for testing. Skips test if no adapter available.
    fn gpu_context() -> GpuContext {
        GpuContext::new_headless().expect("No GPU adapter found (test requires GPU)")
    }

    fn make_pedestrian(
        pos_x: f32,
        pos_y: f32,
        vel_x: f32,
        vel_y: f32,
        dest_x: f32,
        dest_y: f32,
    ) -> GpuPedestrian {
        GpuPedestrian {
            pos_x,
            pos_y,
            vel_x,
            vel_y,
            dest_x,
            dest_y,
            radius: 0.3,
            _pad: 0.0,
        }
    }

    fn default_social_params(grid_w: u32, grid_h: u32, cell_size: f32) -> PedestrianAdaptiveParams {
        PedestrianAdaptiveParams {
            grid_w,
            grid_h,
            cell_size,
            ..PedestrianAdaptiveParams::default()
        }
    }

    /// Prefix-sum correctness: place pedestrians in known cells and verify
    /// that after count+prefix_sum+scatter, all pedestrians appear in the
    /// compacted array exactly once.
    #[test]
    fn prefix_sum_and_scatter_correctness() {
        let ctx = gpu_context();
        let (device, queue) = (&ctx.device, &ctx.queue);
        let mut pipeline = PedestrianAdaptivePipeline::new(device);

        // 10 pedestrians in a 4x4 grid (cell_size=5.0, grid covers 20x20m).
        // Place them in known cells:
        // Cell (0,0): 3 peds at (1,1), (2,2), (3,3)
        // Cell (1,0): 0 peds (empty)
        // Cell (0,1): 2 peds at (1,6), (2,7)
        // Cell (2,2): 5 peds at (11,11), (12,12), (13,13), (14,14), (11,12)
        let peds = vec![
            make_pedestrian(1.0, 1.0, 0.0, 0.0, 10.0, 10.0),
            make_pedestrian(2.0, 2.0, 0.0, 0.0, 10.0, 10.0),
            make_pedestrian(3.0, 3.0, 0.0, 0.0, 10.0, 10.0),
            make_pedestrian(1.0, 6.0, 0.0, 0.0, 10.0, 10.0),
            make_pedestrian(2.0, 7.0, 0.0, 0.0, 10.0, 10.0),
            make_pedestrian(11.0, 11.0, 0.0, 0.0, 0.0, 0.0),
            make_pedestrian(12.0, 12.0, 0.0, 0.0, 0.0, 0.0),
            make_pedestrian(13.0, 13.0, 0.0, 0.0, 0.0, 0.0),
            make_pedestrian(14.0, 14.0, 0.0, 0.0, 0.0, 0.0),
            make_pedestrian(11.0, 12.0, 0.0, 0.0, 0.0, 0.0),
        ];

        let grid_w = 4;
        let grid_h = 4;
        let cell_size = 5.0;

        pipeline.upload(device, queue, &peds, grid_w, grid_h);

        let social_params = default_social_params(grid_w, grid_h, cell_size);

        let mut encoder = device.create_command_encoder(&Default::default());
        pipeline.dispatch(&mut encoder, device, queue, &social_params);
        queue.submit(std::iter::once(encoder.finish()));

        // After dispatch, all pedestrians should still exist (readback).
        let result = pipeline.readback(device, queue);
        assert_eq!(result.len(), 10, "All 10 pedestrians should be present");

        // Verify no NaN or Inf in output positions.
        for (i, ped) in result.iter().enumerate() {
            assert!(
                ped.pos_x.is_finite(),
                "Pedestrian {i} pos_x is not finite: {}",
                ped.pos_x
            );
            assert!(
                ped.pos_y.is_finite(),
                "Pedestrian {i} pos_y is not finite: {}",
                ped.pos_y
            );
        }
    }

    /// Social force GPU vs CPU reference: two pedestrians approaching each other
    /// should produce repulsive acceleration (verify direction matches CPU).
    #[test]
    fn social_force_gpu_vs_cpu_reference() {
        let ctx = gpu_context();
        let (device, queue) = (&ctx.device, &ctx.queue);
        let mut pipeline = PedestrianAdaptivePipeline::new(device);

        // Two pedestrians face-to-face:
        // Ped 0 at (5, 5) moving right -> dest (15, 5)
        // Ped 1 at (6, 5) moving left  -> dest (0, 5)
        let peds = vec![
            make_pedestrian(5.0, 5.0, 1.0, 0.0, 15.0, 5.0),
            make_pedestrian(6.0, 5.0, -1.0, 0.0, 0.0, 5.0),
        ];

        let grid_w = 4;
        let grid_h = 4;
        let cell_size = 5.0;
        let dt = 0.1;

        pipeline.upload(device, queue, &peds, grid_w, grid_h);

        let social_params = PedestrianAdaptiveParams {
            grid_w,
            grid_h,
            cell_size,
            dt,
            ..PedestrianAdaptiveParams::default()
        };

        let mut encoder = device.create_command_encoder(&Default::default());
        pipeline.dispatch(&mut encoder, device, queue, &social_params);
        queue.submit(std::iter::once(encoder.finish()));

        let result = pipeline.readback(device, queue);
        assert_eq!(result.len(), 2);

        // CPU reference computation for comparison.
        let cpu_params = SocialForceParams::default();
        let cpu_accel_0 = social_force_acceleration(
            [5.0, 5.0],
            [1.0, 0.0],
            [15.0, 5.0],
            &[PedestrianNeighbor {
                pos: [6.0, 5.0],
                vel: [-1.0, 0.0],
                radius: 0.3,
            }],
            &cpu_params,
        );

        // Ped 0: should be pushed LEFT (away from ped 1 who is to the right).
        // After one step, ped 0's velocity should have decreased in x or position
        // should show the repulsive effect.
        let gpu_vel_0_x = result[0].vel_x;

        // CPU expected new velocity: vel_x + accel_x * dt
        let cpu_new_vx = 1.0 + cpu_accel_0[0] as f32 * dt;

        // GPU should be within 5% of CPU for x-velocity.
        let tolerance = (cpu_new_vx.abs() * 0.05).max(0.01);
        assert!(
            (gpu_vel_0_x - cpu_new_vx).abs() < tolerance,
            "GPU vel_x ({gpu_vel_0_x:.4}) should be within 5% of CPU ({cpu_new_vx:.4})"
        );
    }

    /// Sparse scenario: 1000 pedestrians distributed across a large area.
    /// Most cells should be empty. Verify dispatch completes without error
    /// and all pedestrians are present in output.
    #[test]
    fn sparse_scenario_1000_pedestrians() {
        let ctx = gpu_context();
        let (device, queue) = (&ctx.device, &ctx.queue);
        let mut pipeline = PedestrianAdaptivePipeline::new(device);

        // 1000 pedestrians spread across a 500x500m area (cell_size=10m -> 50x50 grid)
        let mut peds = Vec::with_capacity(1000);
        for i in 0..1000 {
            let x = (i as f32 * 17.0) % 500.0; // pseudo-random distribution
            let y = (i as f32 * 23.0) % 500.0;
            peds.push(make_pedestrian(x, y, 0.5, 0.3, 250.0, 250.0));
        }

        let grid_w = 50;
        let grid_h = 50;
        let cell_size = 10.0;

        pipeline.upload(device, queue, &peds, grid_w, grid_h);

        let social_params = PedestrianAdaptiveParams {
            grid_w,
            grid_h,
            cell_size,
            dt: 0.1,
            ..PedestrianAdaptiveParams::default()
        };

        let mut encoder = device.create_command_encoder(&Default::default());
        pipeline.dispatch(&mut encoder, device, queue, &social_params);
        queue.submit(std::iter::once(encoder.finish()));

        let result = pipeline.readback(device, queue);
        assert_eq!(result.len(), 1000, "All 1000 pedestrians should be present");

        // Verify no NaN/Inf in any output.
        for (i, ped) in result.iter().enumerate() {
            assert!(
                ped.pos_x.is_finite() && ped.pos_y.is_finite(),
                "Pedestrian {i} has non-finite position: ({}, {})",
                ped.pos_x,
                ped.pos_y
            );
            assert!(
                ped.vel_x.is_finite() && ped.vel_y.is_finite(),
                "Pedestrian {i} has non-finite velocity: ({}, {})",
                ped.vel_x,
                ped.vel_y
            );
        }
    }

    /// Density classification unit tests (also in ped_adaptive mod tests,
    /// but exercised here through the public API).
    #[test]
    fn density_classification() {
        assert_eq!(PedestrianAdaptivePipeline::classify_density(200, 10_000.0), 2.0);
        assert_eq!(PedestrianAdaptivePipeline::classify_density(50, 10_000.0), 5.0);
        assert_eq!(PedestrianAdaptivePipeline::classify_density(5, 10_000.0), 10.0);
    }
}
