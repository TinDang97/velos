//! GPU round-trip integration tests.
//! Gated by `--features gpu-tests`. Tests skip gracefully if no adapter.

#![cfg(feature = "gpu-tests")]

use hecs::World;
use velos_core::components::{Kinematics, Position};
use velos_gpu::{BufferPool, ComputeDispatcher, GpuContext};

fn skip_if_no_gpu() -> Option<GpuContext> {
    GpuContext::new_headless()
}

/// GPU-01: Compute shader dispatches and writes position data back.
#[test]
fn test_compute_dispatch() {
    let ctx = match skip_if_no_gpu() {
        Some(c) => c,
        None => {
            eprintln!("SKIP: no GPU adapter");
            return;
        }
    };

    let mut pool = BufferPool::new(&ctx.device, 256);
    let mut world = World::new();

    // Spawn 10 agents with known state
    for i in 0..10 {
        world.spawn((
            Position {
                x: i as f64 * 10.0,
                y: 0.0,
            },
            Kinematics {
                vx: 5.0,
                vy: 0.0,
                speed: 5.0,
                heading: 0.0,
            },
        ));
    }

    pool.upload_from_ecs(&world, &ctx.queue);
    // Copy back buffer to front so compute reads the uploaded data
    {
        let mut encoder = ctx.device.create_command_encoder(&Default::default());
        let pos_bytes = (pool.agent_count as usize * std::mem::size_of::<[f32; 2]>()) as u64;
        let kin_bytes =
            (pool.agent_count as usize * std::mem::size_of::<[f32; 4]>()) as u64;
        encoder.copy_buffer_to_buffer(&pool.pos_back, 0, &pool.pos_front, 0, pos_bytes);
        encoder.copy_buffer_to_buffer(&pool.kin_back, 0, &pool.kin_front, 0, kin_bytes);
        ctx.queue.submit(std::iter::once(encoder.finish()));
        let _ = ctx.device.poll(wgpu::PollType::wait_indefinitely());
    }

    let dispatcher = ComputeDispatcher::new(&ctx.device);
    let mut encoder = ctx.device.create_command_encoder(&Default::default());
    dispatcher.dispatch(&mut encoder, &pool, &ctx.device, &ctx.queue, 0.1);
    ctx.queue.submit(std::iter::once(encoder.finish()));
    let _ = ctx.device.poll(wgpu::PollType::wait_indefinitely());

    pool.swap();
    let positions = ComputeDispatcher::readback_positions(&pool, &ctx.device, &ctx.queue);

    assert_eq!(positions.len(), 10, "Expected 10 agent positions");
    // Agent 0: x=0 + 5.0 * 0.1 = 0.5
    assert!(
        (positions[0][0] - 0.5_f32).abs() < 0.01,
        "Agent 0 x: expected 0.5, got {}",
        positions[0][0]
    );
}

/// GPU-02: f32 GPU vs f64 CPU results within tolerance.
#[test]
fn test_f32_f64_tolerance() {
    let ctx = match skip_if_no_gpu() {
        Some(c) => c,
        None => {
            eprintln!("SKIP: no GPU adapter");
            return;
        }
    };

    let mut pool = BufferPool::new(&ctx.device, 256);
    let mut world = World::new();

    // Test with positions and velocities at plausible simulation ranges
    let test_cases: Vec<(f64, f64, f64, f64)> = vec![
        (0.0, 0.0, 5.0, 0.0),
        (500.0, 250.0, 13.9, 3.0),
        (999.0, 999.0, 50.0, 0.0),
        (0.1, 0.1, 0.5, 1.5),
    ];

    for (x, y, vx, vy) in &test_cases {
        world.spawn((
            Position { x: *x, y: *y },
            Kinematics {
                vx: *vx,
                vy: *vy,
                speed: (vx * vx + vy * vy).sqrt(),
                heading: vy.atan2(*vx),
            },
        ));
    }

    pool.upload_from_ecs(&world, &ctx.queue);
    {
        let mut encoder = ctx.device.create_command_encoder(&Default::default());
        let pos_bytes = (pool.agent_count as usize * std::mem::size_of::<[f32; 2]>()) as u64;
        let kin_bytes =
            (pool.agent_count as usize * std::mem::size_of::<[f32; 4]>()) as u64;
        encoder.copy_buffer_to_buffer(&pool.pos_back, 0, &pool.pos_front, 0, pos_bytes);
        encoder.copy_buffer_to_buffer(&pool.kin_back, 0, &pool.kin_front, 0, kin_bytes);
        ctx.queue.submit(std::iter::once(encoder.finish()));
        let _ = ctx.device.poll(wgpu::PollType::wait_indefinitely());
    }

    let dt = 0.1_f64;
    let dispatcher = ComputeDispatcher::new(&ctx.device);
    let mut encoder = ctx.device.create_command_encoder(&Default::default());
    dispatcher.dispatch(&mut encoder, &pool, &ctx.device, &ctx.queue, dt as f32);
    ctx.queue.submit(std::iter::once(encoder.finish()));
    let _ = ctx.device.poll(wgpu::PollType::wait_indefinitely());

    pool.swap();
    let gpu_positions = ComputeDispatcher::readback_positions(&pool, &ctx.device, &ctx.queue);

    let epsilon = 1e-4_f64;
    for (i, (x, y, vx, vy)) in test_cases.iter().enumerate() {
        let cpu_x = x + vx * dt;
        let cpu_y = y + vy * dt;
        let gpu_x = gpu_positions[i][0] as f64;
        let gpu_y = gpu_positions[i][1] as f64;

        assert!(
            (cpu_x - gpu_x).abs() < epsilon,
            "Agent {i} x: CPU={cpu_x:.6}, GPU={gpu_x:.6}, diff={:.2e}",
            (cpu_x - gpu_x).abs()
        );
        assert!(
            (cpu_y - gpu_y).abs() < epsilon,
            "Agent {i} y: CPU={cpu_y:.6}, GPU={gpu_y:.6}, diff={:.2e}",
            (cpu_y - gpu_y).abs()
        );
    }
}

/// GPU-03: 1K hecs entities round-trip through GPU buffers correctly.
#[test]
fn test_round_trip_1k() {
    let ctx = match skip_if_no_gpu() {
        Some(c) => c,
        None => {
            eprintln!("SKIP: no GPU adapter");
            return;
        }
    };

    const N: usize = 1000;
    let mut pool = BufferPool::new(&ctx.device, 1024);
    let mut world = World::new();

    // Known initial conditions
    let initial: Vec<(f64, f64, f64, f64)> = (0..N)
        .map(|i| {
            let x = (i as f64) * 1.0;
            let y = (i as f64) * 0.5;
            let vx = 5.0_f64;
            let vy = 2.0_f64;
            (x, y, vx, vy)
        })
        .collect();

    for (x, y, vx, vy) in &initial {
        world.spawn((
            Position { x: *x, y: *y },
            Kinematics {
                vx: *vx,
                vy: *vy,
                speed: (vx * vx + vy * vy).sqrt(),
                heading: vy.atan2(*vx),
            },
        ));
    }

    pool.upload_from_ecs(&world, &ctx.queue);
    {
        let mut encoder = ctx.device.create_command_encoder(&Default::default());
        let pos_bytes = (pool.agent_count as usize * std::mem::size_of::<[f32; 2]>()) as u64;
        let kin_bytes =
            (pool.agent_count as usize * std::mem::size_of::<[f32; 4]>()) as u64;
        encoder.copy_buffer_to_buffer(&pool.pos_back, 0, &pool.pos_front, 0, pos_bytes);
        encoder.copy_buffer_to_buffer(&pool.kin_back, 0, &pool.kin_front, 0, kin_bytes);
        ctx.queue.submit(std::iter::once(encoder.finish()));
        let _ = ctx.device.poll(wgpu::PollType::wait_indefinitely());
    }

    let dt = 0.1_f64;
    let dispatcher = ComputeDispatcher::new(&ctx.device);
    let mut encoder = ctx.device.create_command_encoder(&Default::default());
    dispatcher.dispatch(&mut encoder, &pool, &ctx.device, &ctx.queue, dt as f32);
    ctx.queue.submit(std::iter::once(encoder.finish()));
    let _ = ctx.device.poll(wgpu::PollType::wait_indefinitely());

    pool.swap();
    let gpu_positions = ComputeDispatcher::readback_positions(&pool, &ctx.device, &ctx.queue);

    assert_eq!(gpu_positions.len(), N, "Expected {N} positions from GPU");

    let epsilon = 0.01_f64; // f32 tolerance for positions up to ~1000m
    let (vx, vy) = (5.0_f64, 2.0_f64);
    for (i, (x, y, _, _)) in initial.iter().enumerate() {
        let cpu_x = x + vx * dt;
        let cpu_y = y + vy * dt;
        let gpu_x = gpu_positions[i][0] as f64;
        let gpu_y = gpu_positions[i][1] as f64;

        assert!(
            (cpu_x - gpu_x).abs() < epsilon,
            "Agent {i}: x CPU={cpu_x:.4} GPU={gpu_x:.4} diff={:.4}",
            (cpu_x - gpu_x).abs()
        );
        assert!(
            (cpu_y - gpu_y).abs() < epsilon,
            "Agent {i}: y CPU={cpu_y:.4} GPU={gpu_y:.4} diff={:.4}",
            (cpu_y - gpu_y).abs()
        );
    }
}
