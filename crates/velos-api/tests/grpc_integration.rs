//! Integration tests for the DetectionService gRPC API.
//!
//! Each test spawns an in-process tonic server on a random port, connects
//! a client, and exercises the RegisterCamera, ListCameras, and
//! StreamDetections RPCs.

use std::sync::{Arc, Mutex};

use rstar::RTree;
use tokio::sync::mpsc;

use velos_api::aggregator::DetectionAggregator;
use velos_api::bridge::ApiCommand;
use velos_api::camera::CameraRegistry;
use velos_api::create_detection_service;
use velos_api::proto::velos::v2::detection_service_client::DetectionServiceClient;
use velos_api::proto::velos::v2::{
    DetectionBatch, DetectionEvent, ListCamerasRequest, RegisterCameraRequest,
};
use velos_net::snap::EdgeSegment;
use velos_net::EquirectangularProjection;

/// Build a minimal edge R-tree with a few test edges around HCMC center.
fn make_test_edge_tree() -> RTree<EdgeSegment> {
    let proj = EquirectangularProjection::new(10.7756, 106.7019);

    // Place some edges near HCMC center coordinates
    let (cx, cy) = proj.project(10.7756, 106.7019);
    let segments = vec![
        EdgeSegment {
            edge_id: 1,
            segment_start: [cx - 50.0, cy],
            segment_end: [cx + 50.0, cy],
            offset_along_edge: 0.0,
        },
        EdgeSegment {
            edge_id: 2,
            segment_start: [cx, cy - 30.0],
            segment_end: [cx, cy + 30.0],
            offset_along_edge: 0.0,
        },
        EdgeSegment {
            edge_id: 3,
            segment_start: [cx + 100.0, cy],
            segment_end: [cx + 200.0, cy],
            offset_along_edge: 0.0,
        },
    ];
    RTree::bulk_load(segments)
}

/// Start a gRPC server on a random port, returning the address and a handle
/// to drain API commands from.
async fn start_test_server() -> (String, mpsc::Receiver<ApiCommand>) {
    let (cmd_tx, cmd_rx) = mpsc::channel(256);
    let aggregator = Arc::new(Mutex::new(DetectionAggregator::default()));
    let registry = Arc::new(Mutex::new(CameraRegistry::new()));
    let edge_tree = Arc::new(make_test_edge_tree());
    let projection = Arc::new(EquirectangularProjection::new(10.7756, 106.7019));

    let service = create_detection_service(cmd_tx, aggregator, registry, edge_tree, projection);

    // Bind to a random port
    let listener = tokio::net::TcpListener::bind("[::1]:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let addr_str = format!("http://[::1]:{}", addr.port());

    let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);

    tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(service)
            .serve_with_incoming(incoming)
            .await
            .unwrap();
    });

    // Brief pause for server startup
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    (addr_str, cmd_rx)
}

#[tokio::test]
async fn test_register_camera() {
    let (addr, mut cmd_rx) = start_test_server().await;

    // Drain bridge commands (fire-and-forget, no reply needed)
    tokio::spawn(async move {
        while let Some(_cmd) = cmd_rx.recv().await {}
    });

    let mut client = DetectionServiceClient::connect(addr).await.unwrap();

    let response = client
        .register_camera(RegisterCameraRequest {
            lat: 10.7756,
            lon: 106.7019,
            heading_deg: 90.0,
            fov_deg: 60.0,
            range_m: 100.0,
            name: "test-cam-1".to_string(),
        })
        .await
        .unwrap()
        .into_inner();

    assert!(response.camera_id > 0, "camera_id should be > 0");
    // covered_edge_ids may or may not be populated depending on
    // exact geometry alignment; the key assertion is camera_id > 0.
    // Edge coverage is thoroughly tested in camera.rs unit tests.
}

#[tokio::test]
async fn test_list_cameras() {
    let (addr, mut cmd_rx) = start_test_server().await;

    // Drain bridge commands (fire-and-forget)
    tokio::spawn(async move {
        while let Some(_cmd) = cmd_rx.recv().await {}
    });

    let mut client = DetectionServiceClient::connect(addr).await.unwrap();

    // Register 2 cameras
    for i in 0..2 {
        client
            .register_camera(RegisterCameraRequest {
                lat: 10.7756 + i as f64 * 0.001,
                lon: 106.7019,
                heading_deg: 90.0,
                fov_deg: 60.0,
                range_m: 50.0,
                name: format!("cam-{}", i),
            })
            .await
            .unwrap();
    }

    let response = client
        .list_cameras(ListCamerasRequest {})
        .await
        .unwrap()
        .into_inner();

    assert_eq!(
        response.cameras.len(),
        2,
        "should list both registered cameras"
    );

    let names: Vec<&str> = response.cameras.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"cam-0"), "should contain cam-0");
    assert!(names.contains(&"cam-1"), "should contain cam-1");
}

#[tokio::test]
async fn test_stream_detections() {
    let (addr, mut cmd_rx) = start_test_server().await;

    // Drain bridge commands (all fire-and-forget now)
    tokio::spawn(async move {
        while let Some(_cmd) = cmd_rx.recv().await {}
    });

    let mut client = DetectionServiceClient::connect(addr).await.unwrap();

    // Register a camera first
    let reg = client
        .register_camera(RegisterCameraRequest {
            lat: 10.7756,
            lon: 106.7019,
            heading_deg: 90.0,
            fov_deg: 60.0,
            range_m: 100.0,
            name: "stream-cam".to_string(),
        })
        .await
        .unwrap()
        .into_inner();
    let cam_id = reg.camera_id;

    // Stream 3 detection batches
    let batches = vec![
        DetectionBatch {
            batch_id: 1,
            events: vec![DetectionEvent {
                camera_id: cam_id,
                timestamp_ms: 1000,
                vehicle_class: 1, // MOTORBIKE
                count: 5,
                speed_kmh: Some(30.0),
            }],
        },
        DetectionBatch {
            batch_id: 2,
            events: vec![DetectionEvent {
                camera_id: cam_id,
                timestamp_ms: 2000,
                vehicle_class: 2, // CAR
                count: 3,
                speed_kmh: None,
            }],
        },
        DetectionBatch {
            batch_id: 3,
            events: vec![DetectionEvent {
                camera_id: cam_id,
                timestamp_ms: 3000,
                vehicle_class: 3, // BUS
                count: 1,
                speed_kmh: Some(25.0),
            }],
        },
    ];

    let stream = tokio_stream::iter(batches);
    let mut response_stream = client
        .stream_detections(stream)
        .await
        .unwrap()
        .into_inner();

    let mut acks = Vec::new();
    while let Some(ack) = response_stream.message().await.unwrap() {
        acks.push(ack);
    }

    assert_eq!(acks.len(), 3, "should receive 3 acks for 3 batches");
    for ack in &acks {
        assert_eq!(ack.status, 0, "all acks should have OK status (0)");
    }
    // Verify batch IDs match
    let batch_ids: Vec<u64> = acks.iter().map(|a| a.batch_id).collect();
    assert_eq!(batch_ids, vec![1, 2, 3]);
}

#[tokio::test]
async fn test_unknown_camera_detection() {
    let (addr, _cmd_rx) = start_test_server().await;

    let mut client = DetectionServiceClient::connect(addr).await.unwrap();

    // Stream detection for a camera_id that was never registered
    let batches = vec![DetectionBatch {
        batch_id: 42,
        events: vec![DetectionEvent {
            camera_id: 999, // unregistered
            timestamp_ms: 1000,
            vehicle_class: 1,
            count: 5,
            speed_kmh: None,
        }],
    }];

    let stream = tokio_stream::iter(batches);
    let mut response_stream = client
        .stream_detections(stream)
        .await
        .unwrap()
        .into_inner();

    let ack = response_stream.message().await.unwrap().unwrap();
    assert_eq!(ack.batch_id, 42);
    assert_eq!(
        ack.status, 1,
        "should receive UNKNOWN_CAMERA status (1) for unregistered camera"
    );
}
