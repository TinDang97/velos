//! Benchmarks for GPU dispatch performance.
//! Run with: cargo bench -p velos-gpu --features gpu-tests -- frame_time
//! Requires Metal GPU adapter on macOS.

#![feature(test)]
extern crate test;

#[cfg(feature = "gpu-tests")]
mod gpu_benches {
    use hecs::World;
    use test::Bencher;
    use velos_core::components::{Kinematics, Position};
    use velos_gpu::{BufferPool, ComputeDispatcher, GpuContext};

    fn setup_world(n: usize) -> (World, Vec<(f64, f64)>) {
        let mut world = World::new();
        let mut initial = Vec::with_capacity(n);
        for i in 0..n {
            let x = i as f64 * 1.0;
            let y = i as f64 * 0.5;
            world.spawn((
                Position { x, y },
                Kinematics {
                    vx: 5.0,
                    vy: 2.0,
                    speed: (29.0_f64).sqrt(),
                    heading: (2.0_f64).atan2(5.0),
                },
            ));
            initial.push((x, y));
        }
        (world, initial)
    }

    /// PERF-01: Measures GPU dispatch + submit time per 1K agent step.
    #[bench]
    fn frame_time(b: &mut Bencher) {
        let ctx = match GpuContext::new_headless() {
            Some(c) => c,
            None => {
                eprintln!("SKIP bench: no GPU adapter");
                return;
            }
        };

        const N: usize = 1000;
        let mut pool = BufferPool::new(&ctx.device, 1024);
        let (world, _) = setup_world(N);
        pool.upload_from_ecs(&world, &ctx.queue);

        // Warm up: copy back -> front
        {
            let mut encoder = ctx.device.create_command_encoder(&Default::default());
            let pos_bytes = (pool.agent_count as usize * 8) as u64;
            let kin_bytes = (pool.agent_count as usize * 16) as u64;
            encoder.copy_buffer_to_buffer(&pool.pos_back, 0, &pool.pos_front, 0, pos_bytes);
            encoder.copy_buffer_to_buffer(&pool.kin_back, 0, &pool.kin_front, 0, kin_bytes);
            ctx.queue.submit(std::iter::once(encoder.finish()));
            let _ = ctx.device.poll(wgpu::PollType::wait_indefinitely());
        }

        let dispatcher = ComputeDispatcher::new(&ctx.device);

        b.iter(|| {
            let mut encoder = ctx.device.create_command_encoder(&Default::default());
            dispatcher.dispatch(&mut encoder, &pool, &ctx.device, &ctx.queue, 0.1);
            ctx.queue.submit(std::iter::once(encoder.finish()));
            let _ = ctx.device.poll(wgpu::PollType::wait_indefinitely());
            pool.swap();
        });
    }

    /// PERF-02: Agents per second metric.
    #[bench]
    fn throughput(b: &mut Bencher) {
        let ctx = match GpuContext::new_headless() {
            Some(c) => c,
            None => {
                eprintln!("SKIP bench: no GPU adapter");
                return;
            }
        };

        const N: usize = 1000;
        let mut pool = BufferPool::new(&ctx.device, 1024);
        let (world, _) = setup_world(N);
        pool.upload_from_ecs(&world, &ctx.queue);

        {
            let mut encoder = ctx.device.create_command_encoder(&Default::default());
            let pos_bytes = (pool.agent_count as usize * 8) as u64;
            let kin_bytes = (pool.agent_count as usize * 16) as u64;
            encoder.copy_buffer_to_buffer(&pool.pos_back, 0, &pool.pos_front, 0, pos_bytes);
            encoder.copy_buffer_to_buffer(&pool.kin_back, 0, &pool.kin_front, 0, kin_bytes);
            ctx.queue.submit(std::iter::once(encoder.finish()));
            let _ = ctx.device.poll(wgpu::PollType::wait_indefinitely());
        }

        let dispatcher = ComputeDispatcher::new(&ctx.device);

        // Each iteration processes N agents; bench framework measures ns/iter
        // -> agents/sec = N / (ns_per_iter * 1e-9)
        b.iter(|| {
            let mut encoder = ctx.device.create_command_encoder(&Default::default());
            dispatcher.dispatch(&mut encoder, &pool, &ctx.device, &ctx.queue, 0.1);
            ctx.queue.submit(std::iter::once(encoder.finish()));
            let _ = ctx.device.poll(wgpu::PollType::wait_indefinitely());
            pool.swap();
            test::black_box(N)
        });
    }
}
