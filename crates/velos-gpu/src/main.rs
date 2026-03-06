//! VELOS GPU Pipeline Visual Proof -- entry point.

use velos_gpu::VelosApp;
use winit::event_loop::{ControlFlow, EventLoop};

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();

    let event_loop = EventLoop::new().expect("Failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = VelosApp::new();
    event_loop.run_app(&mut app).expect("Event loop failed");
}
