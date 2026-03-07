//! Tests for signal priority request queue.

use velos_signal::priority::{PriorityLevel, PriorityQueue, PriorityRequest};

#[test]
fn priority_level_ordering_emergency_gt_bus() {
    assert!(PriorityLevel::Emergency > PriorityLevel::Bus);
}

#[test]
fn priority_queue_submit_and_dequeue() {
    let mut queue = PriorityQueue::new();
    let req = PriorityRequest {
        approach_index: 0,
        level: PriorityLevel::Bus,
        vehicle_id: 42,
    };
    queue.submit(req);
    let dequeued = queue.dequeue();
    assert!(dequeued.is_some());
    let r = dequeued.unwrap();
    assert_eq!(r.vehicle_id, 42);
    assert_eq!(r.level, PriorityLevel::Bus);
}

#[test]
fn priority_queue_dequeue_highest_priority() {
    let mut queue = PriorityQueue::new();
    queue.submit(PriorityRequest {
        approach_index: 0,
        level: PriorityLevel::Bus,
        vehicle_id: 1,
    });
    queue.submit(PriorityRequest {
        approach_index: 1,
        level: PriorityLevel::Emergency,
        vehicle_id: 2,
    });
    // Emergency should be dequeued first
    let dequeued = queue.dequeue().unwrap();
    assert_eq!(dequeued.level, PriorityLevel::Emergency);
    assert_eq!(dequeued.vehicle_id, 2);
}

#[test]
fn priority_queue_max_one_per_cycle() {
    let mut queue = PriorityQueue::new();
    queue.submit(PriorityRequest {
        approach_index: 0,
        level: PriorityLevel::Bus,
        vehicle_id: 1,
    });
    queue.submit(PriorityRequest {
        approach_index: 1,
        level: PriorityLevel::Bus,
        vehicle_id: 2,
    });

    // First dequeue succeeds
    assert!(queue.dequeue().is_some());
    // Second dequeue returns None (served this cycle)
    assert!(queue.dequeue().is_none());
}

#[test]
fn priority_queue_reset_cycle_allows_new_dequeue() {
    let mut queue = PriorityQueue::new();
    queue.submit(PriorityRequest {
        approach_index: 0,
        level: PriorityLevel::Bus,
        vehicle_id: 1,
    });
    let _ = queue.dequeue();
    // Already served this cycle
    assert!(queue.dequeue().is_none());

    // Reset cycle
    queue.reset_cycle();
    queue.submit(PriorityRequest {
        approach_index: 0,
        level: PriorityLevel::Bus,
        vehicle_id: 3,
    });
    assert!(queue.dequeue().is_some());
}

#[test]
fn priority_queue_empty_returns_none() {
    let mut queue = PriorityQueue::new();
    assert!(queue.dequeue().is_none());
}
