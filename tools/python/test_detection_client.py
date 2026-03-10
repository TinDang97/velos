#!/usr/bin/env python3
"""Integration test script for the VELOS DetectionService gRPC client.

Requires a running VELOS gRPC server. This is a manual integration test,
not run in CI.

Usage:
    python tools/python/test_detection_client.py
    python tools/python/test_detection_client.py --addr localhost:50051

Prerequisites:
    1. Install dependencies: uv pip install grpcio grpcio-tools protobuf
    2. Generate stubs (from repo root):
       python -m grpc_tools.protoc \
           --proto_path=proto \
           --python_out=tools/python \
           --pyi_out=tools/python \
           --grpc_python_out=tools/python \
           proto/velos/v2/detection.proto
    3. Start VELOS: cargo run -p velos-gpu
"""

from __future__ import annotations

import argparse
import sys
import time

# Ensure tools/python is on the path for generated stubs
sys.path.insert(0, ".")

from detection_client import VelosDetectionClient, make_detection_event

# Generated stubs
from velos.v2 import detection_pb2


# HCMC test coordinates (District 1 area)
HCMC_LAT = 10.7756
HCMC_LON = 106.7019

# VehicleClass enum values from proto
MOTORBIKE = 1
CAR = 2
BUS = 3
TRUCK = 4
BICYCLE = 5
PEDESTRIAN = 6

VEHICLE_NAMES = {
    MOTORBIKE: "Motorbike",
    CAR: "Car",
    BUS: "Bus",
    TRUCK: "Truck",
    BICYCLE: "Bicycle",
    PEDESTRIAN: "Pedestrian",
}


def test_register_camera(client: VelosDetectionClient) -> int:
    """Register a test camera and return its ID."""
    print("\n--- Test: Register Camera ---")
    camera_id, covered_edges = client.register_camera(
        lat=HCMC_LAT,
        lon=HCMC_LON,
        heading_deg=90.0,
        fov_deg=60.0,
        range_m=40.0,
        name="hcmc-test-cam",
    )
    print(f"  Registered camera ID: {camera_id}")
    print(f"  Covered edges: {covered_edges}")
    assert camera_id > 0, "Camera ID should be positive"
    print("  PASSED")
    return camera_id


def test_list_cameras(client: VelosDetectionClient) -> None:
    """List cameras and verify the registered camera appears."""
    print("\n--- Test: List Cameras ---")
    cameras = client.list_cameras()
    print(f"  Found {len(cameras)} camera(s):")
    for cam in cameras:
        print(f"    [{cam.camera_id}] {cam.name} at ({cam.lat:.4f}, {cam.lon:.4f})")
    assert len(cameras) >= 1, "Should have at least 1 camera"
    names = [c.name for c in cameras]
    assert "hcmc-test-cam" in names, "Registered camera should appear in list"
    print("  PASSED")


def test_stream_detections(
    client: VelosDetectionClient, camera_id: int
) -> None:
    """Stream 5 detection batches with mixed vehicle classes."""
    print("\n--- Test: Stream Detections ---")

    now_ms = int(time.time() * 1000)

    batches = []
    for i in range(5):
        events = [
            make_detection_event(camera_id, MOTORBIKE, 10 + i, speed_kmh=35.0, timestamp_ms=now_ms + i * 1000),
            make_detection_event(camera_id, CAR, 3 + i, speed_kmh=40.0, timestamp_ms=now_ms + i * 1000),
            make_detection_event(camera_id, BUS, 1, timestamp_ms=now_ms + i * 1000),
        ]
        if i % 2 == 0:
            events.append(
                make_detection_event(camera_id, BICYCLE, 2, speed_kmh=15.0, timestamp_ms=now_ms + i * 1000)
            )
        batches.append(
            detection_pb2.DetectionBatch(batch_id=i + 1, events=events)
        )

    print(f"  Streaming {len(batches)} batches...")
    acks = list(client.stream_detections(batches))

    print(f"  Received {len(acks)} ack(s):")
    total_events = 0
    for ack in acks:
        status_name = "OK" if ack.status == 0 else f"ERROR({ack.status})"
        batch = batches[ack.batch_id - 1]
        event_count = len(batch.events)
        total_events += event_count
        print(f"    Batch {ack.batch_id}: {status_name} ({event_count} events)")

    assert len(acks) == 5, f"Expected 5 acks, got {len(acks)}"
    assert all(a.status == 0 for a in acks), "All acks should be OK"
    print(f"  Total events streamed: {total_events}")
    print("  PASSED")

    # Print aggregation summary
    print("\n--- Aggregation Summary ---")
    class_counts: dict[int, int] = {}
    class_speeds: dict[int, list[float]] = {}
    for batch in batches:
        for event in batch.events:
            vc = event.vehicle_class
            class_counts[vc] = class_counts.get(vc, 0) + event.count
            if event.HasField("speed_kmh"):
                class_speeds.setdefault(vc, []).extend(
                    [event.speed_kmh] * event.count
                )

    for vc in sorted(class_counts.keys()):
        name = VEHICLE_NAMES.get(vc, f"Class_{vc}")
        count = class_counts[vc]
        speeds = class_speeds.get(vc, [])
        avg_speed = sum(speeds) / len(speeds) if speeds else 0
        speed_str = f", avg speed: {avg_speed:.1f} km/h" if speeds else ""
        print(f"  {name}: {count} detections{speed_str}")


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Test VELOS DetectionService gRPC client"
    )
    parser.add_argument(
        "--addr",
        default="localhost:50051",
        help="gRPC server address (default: localhost:50051)",
    )
    args = parser.parse_args()

    print(f"Connecting to VELOS gRPC server at {args.addr}...")

    try:
        with VelosDetectionClient(args.addr) as client:
            camera_id = test_register_camera(client)
            test_list_cameras(client)
            test_stream_detections(client, camera_id)

        print("\n=== All tests PASSED ===")
    except Exception as e:
        print(f"\n=== FAILED: {e} ===")
        sys.exit(1)


if __name__ == "__main__":
    main()
