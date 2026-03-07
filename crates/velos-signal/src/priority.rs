//! Signal priority request queue for buses and emergency vehicles.
//!
//! Provides a priority queue that accepts requests from transit and
//! emergency vehicles, dequeuing the highest-priority request per cycle.
//! Emergency vehicles always override bus priority.

/// Priority level for signal priority requests.
///
/// Emergency vehicles have higher priority than buses.
/// Ordering: `Emergency > Bus`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PriorityLevel {
    /// Transit bus priority (lower priority).
    Bus = 0,
    /// Emergency vehicle priority (higher priority).
    Emergency = 1,
}

/// A signal priority request from a vehicle.
#[derive(Debug, Clone)]
pub struct PriorityRequest {
    /// Which approach the requesting vehicle is on.
    pub approach_index: usize,
    /// Priority level of the request.
    pub level: PriorityLevel,
    /// Unique ID of the requesting vehicle.
    pub vehicle_id: u32,
}

/// Signal priority request queue.
///
/// Accepts priority requests from vehicles and dequeues the highest-priority
/// request once per cycle. After serving one request, further dequeue attempts
/// return `None` until `reset_cycle()` is called.
#[derive(Debug)]
pub struct PriorityQueue {
    requests: Vec<PriorityRequest>,
    served_this_cycle: bool,
}

impl PriorityQueue {
    /// Create an empty priority queue.
    pub fn new() -> Self {
        Self {
            requests: Vec::new(),
            served_this_cycle: false,
        }
    }

    /// Submit a priority request to the queue.
    pub fn submit(&mut self, request: PriorityRequest) {
        self.requests.push(request);
    }

    /// Dequeue the highest-priority request.
    ///
    /// Returns `None` if the queue is empty or a request has already been
    /// served this cycle (max 1 per cycle to prevent starvation).
    pub fn dequeue(&mut self) -> Option<PriorityRequest> {
        if self.served_this_cycle || self.requests.is_empty() {
            return None;
        }

        // Find the highest-priority request
        let best_idx = self
            .requests
            .iter()
            .enumerate()
            .max_by_key(|(_, r)| r.level)
            .map(|(i, _)| i)?;

        let request = self.requests.swap_remove(best_idx);
        self.served_this_cycle = true;
        Some(request)
    }

    /// Reset the cycle flag, allowing a new dequeue in the next cycle.
    pub fn reset_cycle(&mut self) {
        self.served_this_cycle = false;
        // Clear any stale requests from the previous cycle
        self.requests.clear();
    }

    /// Check if a request has been served this cycle.
    pub fn served_this_cycle(&self) -> bool {
        self.served_this_cycle
    }

    /// Number of pending requests in the queue.
    pub fn len(&self) -> usize {
        self.requests.len()
    }

    /// Whether the queue is empty.
    pub fn is_empty(&self) -> bool {
        self.requests.is_empty()
    }
}

impl Default for PriorityQueue {
    fn default() -> Self {
        Self::new()
    }
}

/// Maximum green extension for priority requests (seconds).
pub const MAX_GREEN_EXTENSION: f64 = 15.0;

/// Maximum red shortening (conflicting green reduction) for priority (seconds).
pub const MAX_RED_SHORTENING: f64 = 10.0;
