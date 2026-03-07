//! CCH weight customization via bottom-up triangle enumeration.
//!
//! Given original edge weights, propagates them through the CCH shortcut
//! graph so that queries return correct shortest-path distances.
//!
//! The algorithm processes nodes bottom-up by rank. For each node v at rank r,
//! it enumerates all pairs of upward edges (r, w1) and (r, w2). If a direct
//! shortcut edge (w1, w2) exists (where w1 < w2), its weight is updated:
//!   forward_weight(w1, w2) = min(current, cost(w1 -> r) + cost(r -> w2))
//!   backward_weight(w1, w2) = min(current, cost(w2 -> r) + cost(r -> w1))

use crate::cch::CCHRouter;

impl CCHRouter {
    /// Customize CCH edge weights from a slice of original directed edge weights.
    ///
    /// `original_weights[i]` is the weight (travel time) of original directed
    /// edge `i` in the petgraph `DiGraph`.
    pub fn customize(&mut self, original_weights: &[f32]) {
        self.customize_with_fn(|i| original_weights[i]);
    }

    /// Customize with a closure that maps original edge index to weight.
    ///
    /// Useful for integrating prediction overlays that modify edge costs
    /// on the fly without materializing a full weight vector.
    pub fn customize_with_fn(&mut self, weight_fn: impl Fn(usize) -> f32) {
        let n = self.node_count;

        // Step 1: Reset all weights to INFINITY.
        for w in self.forward_weight.iter_mut() {
            *w = f32::INFINITY;
        }
        for w in self.backward_weight.iter_mut() {
            *w = f32::INFINITY;
        }

        // Step 2: Initialize CCH edges from original graph weights.
        for orig_idx in 0..self.original_edge_to_cch.len() {
            let cch_idx = self.original_edge_to_cch[orig_idx];
            let w = weight_fn(orig_idx);
            self.forward_weight[cch_idx] = self.forward_weight[cch_idx].min(w);
        }

        // Copy forward weights to corresponding backward positions.
        for low_rank in 0..n {
            let fw_begin = self.forward_first_out[low_rank] as usize;
            let fw_end = self.forward_first_out[low_rank + 1] as usize;
            for fw_pos in fw_begin..fw_end {
                let high_rank = self.forward_head[fw_pos];
                if let Some(bw_pos) = self.find_backward_pos(high_rank, low_rank as u32) {
                    self.backward_weight[bw_pos] = self.forward_weight[fw_pos];
                }
            }
        }

        // Step 3: Bottom-up triangle enumeration.
        //
        // For each node at rank r (lowest first), enumerate pairs of upward
        // edges (r, w1) and (r, w2). If shortcut (min(w1,w2), max(w1,w2))
        // exists, update its weight.
        //
        // Using sorted merge for O(d * log(d)) per node instead of O(d^2 * log(d)).
        // Actually we need all pairs, so it's O(d^2) minimum. But we avoid
        // repeated binary searches by caching edge positions.

        for rank in 0..n as u32 {
            let fw_begin = self.forward_first_out[rank as usize] as usize;
            let fw_end = self.forward_first_out[rank as usize + 1] as usize;
            let degree = fw_end - fw_begin;
            if degree < 2 {
                continue;
            }

            // Collect forward edges: (target_rank, fw_weight, bw_weight)
            // fw_weight = cost(rank -> target) = forward_weight[pos]
            // bw_weight = cost(target -> rank) = backward_weight at backward_star[target] for rank
            //           = forward_weight[pos] for symmetric graphs
            let edges: Vec<(u32, f32, f32)> = (fw_begin..fw_end)
                .map(|pos| {
                    let target = self.forward_head[pos];
                    let fw = self.forward_weight[pos];
                    let bw = self
                        .find_backward_pos(target, rank)
                        .map(|bw_pos| self.backward_weight[bw_pos])
                        .unwrap_or(fw);
                    (target, fw, bw)
                })
                .collect();

            for i in 0..edges.len() {
                for j in (i + 1)..edges.len() {
                    let (w1, fw_r_w1, bw_w1_r) = edges[i];
                    let (w2, fw_r_w2, bw_w2_r) = edges[j];

                    // Ensure lo < hi
                    let (lo, hi, fw_r_lo, fw_r_hi, bw_lo_r, bw_hi_r) = if w1 < w2 {
                        (w1, w2, fw_r_w1, fw_r_w2, bw_w1_r, bw_w2_r)
                    } else {
                        (w2, w1, fw_r_w2, fw_r_w1, bw_w2_r, bw_w1_r)
                    };

                    // Forward: lo -> rank -> hi = cost(lo->rank) + cost(rank->hi)
                    let via_forward = bw_lo_r + fw_r_hi;
                    // Backward: hi -> rank -> lo = cost(hi->rank) + cost(rank->lo)
                    let via_backward = bw_hi_r + fw_r_lo;

                    // Update forward_weight of edge (lo, hi)
                    if let Some(fw_idx) = self.find_forward_pos(lo, hi)
                        && via_forward < self.forward_weight[fw_idx]
                    {
                        self.forward_weight[fw_idx] = via_forward;
                    }

                    // Update backward_weight of edge (lo, hi)
                    if let Some(bw_idx) = self.find_backward_pos(hi, lo)
                        && via_backward < self.backward_weight[bw_idx]
                    {
                        self.backward_weight[bw_idx] = via_backward;
                    }
                }
            }
        }
    }

    /// Find the position in forward star of `from_rank` for edge to `to_rank`.
    fn find_forward_pos(&self, from_rank: u32, to_rank: u32) -> Option<usize> {
        let begin = self.forward_first_out[from_rank as usize] as usize;
        let end = self.forward_first_out[from_rank as usize + 1] as usize;
        let slice = &self.forward_head[begin..end];
        slice
            .binary_search(&to_rank)
            .ok()
            .map(|offset| begin + offset)
    }

    /// Find the position in backward star of `at_rank` for edge from `from_rank`.
    fn find_backward_pos(&self, at_rank: u32, from_rank: u32) -> Option<usize> {
        let begin = self.backward_first_out[at_rank as usize] as usize;
        let end = self.backward_first_out[at_rank as usize + 1] as usize;
        let slice = &self.backward_head[begin..end];
        slice
            .binary_search(&from_rank)
            .ok()
            .map(|offset| begin + offset)
    }
}
