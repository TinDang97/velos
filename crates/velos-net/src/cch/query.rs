//! Bidirectional Dijkstra query on customized CCH.
//!
//! After weight customization, queries find shortest paths by running
//! simultaneous forward and backward searches on the upward-only CCH graph,
//! meeting at the optimal node.
//!
//! - `query`: returns shortest-path cost only
//! - `query_with_path`: returns cost + unpacked node sequence
//! - `query_batch`: parallel queries via rayon

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use rayon::prelude::*;

use crate::cch::CCHRouter;

/// Entry in the min-heap for Dijkstra (ordered by cost).
#[derive(Debug, Clone, PartialEq)]
struct DijkstraEntry {
    cost: f32,
    rank: u32,
}

impl Eq for DijkstraEntry {}

impl PartialOrd for DijkstraEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DijkstraEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.cost
            .partial_cmp(&other.cost)
            .unwrap_or(std::cmp::Ordering::Equal)
    }
}

impl CCHRouter {
    /// Query the shortest-path cost between two nodes (by original node index).
    ///
    /// Returns `None` if no path exists. Returns `Some(0.0)` if source == target.
    pub fn query(&self, source: u32, target: u32) -> Option<f32> {
        if source == target {
            return Some(0.0);
        }
        if source as usize >= self.node_count || target as usize >= self.node_count {
            return None;
        }

        let source_rank = self.node_order[source as usize];
        let target_rank = self.node_order[target as usize];

        self.query_by_rank(source_rank, target_rank)
    }

    /// Query the shortest-path cost and return the unpacked path as original node indices.
    ///
    /// The path starts at `source` and ends at `target`. Shortcuts are recursively
    /// expanded to produce a sequence of consecutive original graph edges.
    pub fn query_with_path(&self, source: u32, target: u32) -> Option<(f32, Vec<u32>)> {
        if source == target {
            return Some((0.0, vec![source]));
        }
        if source as usize >= self.node_count || target as usize >= self.node_count {
            return None;
        }

        let source_rank = self.node_order[source as usize];
        let target_rank = self.node_order[target as usize];

        let (cost, parent_forward, parent_backward, meeting_rank) =
            self.bidirectional_search_with_parents(source_rank, target_rank)?;

        // Reconstruct rank-space path: source -> meeting -> target
        let mut fw_segment = Vec::new();
        let mut r = meeting_rank;
        while r != u32::MAX {
            fw_segment.push(r);
            r = parent_forward[r as usize];
        }
        fw_segment.reverse();

        let mut rank_path = fw_segment;
        r = parent_backward[meeting_rank as usize];
        while r != u32::MAX {
            rank_path.push(r);
            r = parent_backward[r as usize];
        }

        // Unpack shortcuts, then convert ranks to node indices
        let unpacked = self.unpack_rank_path(&rank_path);
        let node_path: Vec<u32> = unpacked
            .iter()
            .map(|&r| self.rank_to_node[r as usize])
            .collect();

        Some((cost, node_path))
    }

    /// Run parallel batch queries using rayon.
    ///
    /// Each query is independent, achieving perfect data parallelism.
    pub fn query_batch(&self, pairs: &[(u32, u32)]) -> Vec<Option<f32>> {
        pairs
            .par_iter()
            .map(|&(s, t)| self.query(s, t))
            .collect()
    }

    /// Core bidirectional Dijkstra on the upward CCH graph.
    ///
    /// Both forward (from source) and backward (from target) searches explore
    /// only upward edges. They meet at the optimal node in the hierarchy.
    /// For symmetric graphs, both directions use `forward_weight`.
    fn query_by_rank(&self, source_rank: u32, target_rank: u32) -> Option<f32> {
        let n = self.node_count;
        let mut forward_dist = vec![f32::INFINITY; n];
        let mut backward_dist = vec![f32::INFINITY; n];

        forward_dist[source_rank as usize] = 0.0;
        backward_dist[target_rank as usize] = 0.0;

        let mut forward_heap: BinaryHeap<Reverse<DijkstraEntry>> = BinaryHeap::new();
        let mut backward_heap: BinaryHeap<Reverse<DijkstraEntry>> = BinaryHeap::new();

        forward_heap.push(Reverse(DijkstraEntry {
            cost: 0.0,
            rank: source_rank,
        }));
        backward_heap.push(Reverse(DijkstraEntry {
            cost: 0.0,
            rank: target_rank,
        }));

        let mut best = f32::INFINITY;

        loop {
            let fw_min = forward_heap
                .peek()
                .map(|e| e.0.cost)
                .unwrap_or(f32::INFINITY);
            let bw_min = backward_heap
                .peek()
                .map(|e| e.0.cost)
                .unwrap_or(f32::INFINITY);

            if fw_min >= best && bw_min >= best {
                break;
            }

            if fw_min <= bw_min {
                if let Some(Reverse(entry)) = forward_heap.pop() {
                    if entry.cost > forward_dist[entry.rank as usize] {
                        continue;
                    }
                    let total = entry.cost + backward_dist[entry.rank as usize];
                    if total < best {
                        best = total;
                    }
                    self.relax_upward(
                        entry.rank,
                        entry.cost,
                        &mut forward_dist,
                        &mut forward_heap,
                    );
                }
            } else if let Some(Reverse(entry)) = backward_heap.pop() {
                if entry.cost > backward_dist[entry.rank as usize] {
                    continue;
                }
                let total = forward_dist[entry.rank as usize] + entry.cost;
                if total < best {
                    best = total;
                }
                self.relax_upward(
                    entry.rank,
                    entry.cost,
                    &mut backward_dist,
                    &mut backward_heap,
                );
            }
        }

        if best.is_finite() {
            Some(best)
        } else {
            None
        }
    }

    /// Relax all upward edges from `rank`, updating distances and heap.
    fn relax_upward(
        &self,
        rank: u32,
        current_cost: f32,
        dist: &mut [f32],
        heap: &mut BinaryHeap<Reverse<DijkstraEntry>>,
    ) {
        let begin = self.forward_first_out[rank as usize] as usize;
        let end = self.forward_first_out[rank as usize + 1] as usize;
        for pos in begin..end {
            let neighbor = self.forward_head[pos];
            let new_cost = current_cost + self.forward_weight[pos];
            if new_cost < dist[neighbor as usize] {
                dist[neighbor as usize] = new_cost;
                heap.push(Reverse(DijkstraEntry {
                    cost: new_cost,
                    rank: neighbor,
                }));
            }
        }
    }

    /// Bidirectional search with parent tracking for path reconstruction.
    fn bidirectional_search_with_parents(
        &self,
        source_rank: u32,
        target_rank: u32,
    ) -> Option<(f32, Vec<u32>, Vec<u32>, u32)> {
        let n = self.node_count;
        let mut forward_dist = vec![f32::INFINITY; n];
        let mut backward_dist = vec![f32::INFINITY; n];
        let mut parent_forward = vec![u32::MAX; n];
        let mut parent_backward = vec![u32::MAX; n];

        forward_dist[source_rank as usize] = 0.0;
        backward_dist[target_rank as usize] = 0.0;

        let mut forward_heap: BinaryHeap<Reverse<DijkstraEntry>> = BinaryHeap::new();
        let mut backward_heap: BinaryHeap<Reverse<DijkstraEntry>> = BinaryHeap::new();

        forward_heap.push(Reverse(DijkstraEntry {
            cost: 0.0,
            rank: source_rank,
        }));
        backward_heap.push(Reverse(DijkstraEntry {
            cost: 0.0,
            rank: target_rank,
        }));

        let mut best = f32::INFINITY;
        let mut meeting_rank = u32::MAX;

        loop {
            let fw_min = forward_heap
                .peek()
                .map(|e| e.0.cost)
                .unwrap_or(f32::INFINITY);
            let bw_min = backward_heap
                .peek()
                .map(|e| e.0.cost)
                .unwrap_or(f32::INFINITY);

            if fw_min >= best && bw_min >= best {
                break;
            }

            if fw_min <= bw_min {
                if let Some(Reverse(entry)) = forward_heap.pop() {
                    if entry.cost > forward_dist[entry.rank as usize] {
                        continue;
                    }
                    let total = entry.cost + backward_dist[entry.rank as usize];
                    if total < best {
                        best = total;
                        meeting_rank = entry.rank;
                    }
                    self.relax_upward_with_parents(
                        entry.rank,
                        entry.cost,
                        &mut forward_dist,
                        &mut parent_forward,
                        &mut forward_heap,
                    );
                }
            } else if let Some(Reverse(entry)) = backward_heap.pop() {
                if entry.cost > backward_dist[entry.rank as usize] {
                    continue;
                }
                let total = forward_dist[entry.rank as usize] + entry.cost;
                if total < best {
                    best = total;
                    meeting_rank = entry.rank;
                }
                self.relax_upward_with_parents(
                    entry.rank,
                    entry.cost,
                    &mut backward_dist,
                    &mut parent_backward,
                    &mut backward_heap,
                );
            }
        }

        if best.is_finite() {
            Some((best, parent_forward, parent_backward, meeting_rank))
        } else {
            None
        }
    }

    /// Relax upward edges with parent pointer tracking.
    fn relax_upward_with_parents(
        &self,
        rank: u32,
        current_cost: f32,
        dist: &mut [f32],
        parents: &mut [u32],
        heap: &mut BinaryHeap<Reverse<DijkstraEntry>>,
    ) {
        let begin = self.forward_first_out[rank as usize] as usize;
        let end = self.forward_first_out[rank as usize + 1] as usize;
        for pos in begin..end {
            let neighbor = self.forward_head[pos];
            let new_cost = current_cost + self.forward_weight[pos];
            if new_cost < dist[neighbor as usize] {
                dist[neighbor as usize] = new_cost;
                parents[neighbor as usize] = rank;
                heap.push(Reverse(DijkstraEntry {
                    cost: new_cost,
                    rank: neighbor,
                }));
            }
        }
    }

    /// Unpack a path of ranks, expanding shortcuts into original edge sequences.
    fn unpack_rank_path(&self, rank_path: &[u32]) -> Vec<u32> {
        if rank_path.is_empty() {
            return vec![];
        }
        let mut result = vec![rank_path[0]];
        for w in rank_path.windows(2) {
            self.unpack_edge(w[0], w[1], &mut result);
        }
        result
    }

    /// Unpack a single edge (from_rank, to_rank), adding intermediate nodes to result.
    /// The `from_rank` is already in result; `to_rank` and any intermediates are appended.
    fn unpack_edge(&self, from_rank: u32, to_rank: u32, result: &mut Vec<u32>) {
        let (lo, hi) = if from_rank < to_rank {
            (from_rank, to_rank)
        } else {
            (to_rank, from_rank)
        };

        let begin = self.forward_first_out[lo as usize] as usize;
        let end = self.forward_first_out[lo as usize + 1] as usize;
        let slice = &self.forward_head[begin..end];

        if let Ok(offset) = slice.binary_search(&hi) {
            let pos = begin + offset;
            match self.shortcut_middle[pos] {
                None => {
                    result.push(to_rank);
                }
                Some(mid_rank) => {
                    self.unpack_edge(from_rank, mid_rank, result);
                    self.unpack_edge(mid_rank, to_rank, result);
                }
            }
        } else {
            // Edge not found -- should not happen in valid CCH path
            result.push(to_rank);
        }
    }
}
