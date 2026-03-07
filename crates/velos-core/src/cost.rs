//! Multi-factor cost function and agent profiles for route choice differentiation.
//!
//! Implements a 6-factor weighted cost function (time, comfort, safety, fuel,
//! signal delay, prediction penalty) with 8 distinct agent profiles. Each
//! profile differs only in weight values -- all share the same CostWeights struct.
//!
//! Profile ID is encoded in bits 4-7 of GpuAgentState.flags for GPU-side lookup.

/// Agent profile type for route cost differentiation.
///
/// Each profile maps to a distinct set of cost weights in [`PROFILE_WEIGHTS`].
/// Stored in 4 bits (0-7) of GpuAgentState.flags (bits 4-7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum AgentProfile {
    /// Time-focused commuter (default for Car/Motorbike).
    Commuter = 0,
    /// Public transit bus -- schedule adherence + safety.
    Bus = 1,
    /// Heavy goods -- fuel cost dominant.
    Truck = 2,
    /// Emergency vehicle -- pure time minimization.
    Emergency = 3,
    /// Tourist -- comfort and safety, not rushed.
    Tourist = 4,
    /// Teen rider -- time-focused, low safety concern.
    Teen = 5,
    /// Senior -- safety and comfort paramount.
    Senior = 6,
    /// Cyclist -- safety paramount, fuel irrelevant.
    Cyclist = 7,
}

impl AgentProfile {
    /// Convert a u8 value to an AgentProfile, returning None for invalid values.
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Commuter),
            1 => Some(Self::Bus),
            2 => Some(Self::Truck),
            3 => Some(Self::Emergency),
            4 => Some(Self::Tourist),
            5 => Some(Self::Teen),
            6 => Some(Self::Senior),
            7 => Some(Self::Cyclist),
            _ => None,
        }
    }
}

/// Weighted cost factors for route evaluation.
///
/// All 6 weights should sum to approximately 1.0 for each profile.
/// The cost function computes: sum_i(weight_i * factor_i) across all edges.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CostWeights {
    /// Weight for travel time factor.
    pub time: f32,
    /// Weight for ride comfort (road quality, turns, noise).
    pub comfort: f32,
    /// Weight for safety score (accident risk, lighting, width).
    pub safety: f32,
    /// Weight for fuel/energy consumption.
    pub fuel: f32,
    /// Weight for expected signal delay penalty.
    pub signal_delay: f32,
    /// Weight for prediction uncertainty penalty.
    pub prediction_penalty: f32,
}

impl CostWeights {
    /// Sum of all weight factors.
    pub fn sum(&self) -> f32 {
        self.time + self.comfort + self.safety + self.fuel + self.signal_delay + self.prediction_penalty
    }
}

/// Lookup table of cost weights for all 8 agent profiles.
///
/// Index by `AgentProfile as usize`. Values from architecture research document.
pub const PROFILE_WEIGHTS: [CostWeights; 8] = [
    // Commuter: time-focused, moderate fuel concern
    CostWeights { time: 0.40, comfort: 0.05, safety: 0.10, fuel: 0.20, signal_delay: 0.15, prediction_penalty: 0.10 },
    // Bus: schedule adherence (time), safety paramount
    CostWeights { time: 0.35, comfort: 0.05, safety: 0.25, fuel: 0.15, signal_delay: 0.10, prediction_penalty: 0.10 },
    // Truck: fuel-heavy (weight = cost), avoids tight roads
    CostWeights { time: 0.20, comfort: 0.10, safety: 0.15, fuel: 0.35, signal_delay: 0.10, prediction_penalty: 0.10 },
    // Emergency: pure time, ignore comfort/fuel
    CostWeights { time: 0.80, comfort: 0.00, safety: 0.05, fuel: 0.00, signal_delay: 0.10, prediction_penalty: 0.05 },
    // Tourist: comfort and safety, not rushed
    CostWeights { time: 0.10, comfort: 0.35, safety: 0.30, fuel: 0.05, signal_delay: 0.05, prediction_penalty: 0.15 },
    // Teen: time-focused, low safety concern (risky behavior)
    CostWeights { time: 0.45, comfort: 0.10, safety: 0.05, fuel: 0.15, signal_delay: 0.15, prediction_penalty: 0.10 },
    // Senior: safety and comfort, slow and steady
    CostWeights { time: 0.10, comfort: 0.25, safety: 0.40, fuel: 0.10, signal_delay: 0.05, prediction_penalty: 0.10 },
    // Cyclist: safety paramount, comfort matters, fuel irrelevant
    CostWeights { time: 0.15, comfort: 0.20, safety: 0.45, fuel: 0.00, signal_delay: 0.10, prediction_penalty: 0.10 },
];

/// Per-edge attributes used by the cost function.
///
/// These are derived from road class heuristics (see [`default_edge_attributes`])
/// or from real-world data when available.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeAttributes {
    /// Current observed or free-flow travel time (seconds).
    pub current_travel_time: f32,
    /// Edge length in metres.
    pub distance_m: f32,
    /// Comfort penalty (0.0 = very comfortable, 1.0 = very uncomfortable).
    pub comfort_penalty: f32,
    /// Safety score (0.0 = very safe, 1.0 = very unsafe).
    pub safety_score: f32,
    /// Fuel consumption rate (litres per metre).
    pub fuel_rate: f32,
    /// Expected signal delay on this edge (seconds).
    pub signal_delay: f32,
}

/// Compute the weighted route cost across a sequence of edges.
///
/// Uses distance-weighted blending: edges within 2000m cumulative distance
/// from agent start use `current_travel_time` (observed). Faraway edges blend
/// toward `overlay_travel_times` (predicted). Low confidence predictions
/// (< 0.5) incur the prediction_penalty.
///
/// # Arguments
/// - `edges`: Edge indices along the route
/// - `overlay_travel_times`: Predicted travel times per edge (from prediction ensemble)
/// - `overlay_confidence`: Confidence scores per prediction (0.0-1.0)
/// - `weights`: Agent profile cost weights
/// - `edge_attrs`: Per-edge attribute data
pub fn route_cost(
    edges: &[u32],
    overlay_travel_times: &[f32],
    overlay_confidence: &[f32],
    weights: &CostWeights,
    edge_attrs: &[EdgeAttributes],
) -> f32 {
    let mut total_cost = 0.0_f32;
    let mut cumulative_distance = 0.0_f32;
    const BLEND_DISTANCE: f32 = 2000.0;

    for &edge_id in edges.iter() {
        let idx = edge_id as usize;
        let attr = &edge_attrs[idx];

        // Distance-weighted blend: nearby uses observed, faraway uses predicted
        let blend = (cumulative_distance / BLEND_DISTANCE).min(1.0);
        let predicted_time = overlay_travel_times[idx];
        let travel_time = attr.current_travel_time * (1.0 - blend)
            + predicted_time * blend;

        // Per-edge cost factors
        let time_cost = travel_time;
        let comfort_cost = attr.comfort_penalty * attr.distance_m;
        let safety_cost = attr.safety_score * attr.distance_m;
        let fuel_cost = attr.fuel_rate * attr.distance_m;
        let signal_cost = attr.signal_delay;

        // Prediction penalty for low-confidence edges
        let confidence = overlay_confidence[idx];
        let pred_penalty = if confidence < 0.5 {
            (0.5 - confidence) * 2.0 * travel_time
        } else {
            0.0
        };

        let edge_cost = weights.time * time_cost
            + weights.comfort * comfort_cost
            + weights.safety * safety_cost
            + weights.fuel * fuel_cost
            + weights.signal_delay * signal_cost
            + weights.prediction_penalty * pred_penalty;

        total_cost += edge_cost;
        cumulative_distance += attr.distance_m;
    }

    total_cost
}

/// Encode an agent profile ID into GpuAgentState flags (bits 4-7).
///
/// Preserves existing bits 0-3 (bit0=at_bus_stop, bit1=emergency_active,
/// bit2=yielding, bit3=reserved).
#[inline]
pub fn encode_profile_in_flags(flags: u32, profile: AgentProfile) -> u32 {
    (flags & 0x0F) | ((profile as u32) << 4)
}

/// Decode an agent profile ID from GpuAgentState flags (bits 4-7).
///
/// Returns the AgentProfile enum variant. Panics if bits 4-7 contain
/// an invalid profile value (> 7).
#[inline]
pub fn decode_profile_from_flags(flags: u32) -> AgentProfile {
    let id = ((flags >> 4) & 0x0F) as u8;
    AgentProfile::from_u8(id).expect("invalid profile ID in flags bits 4-7")
}

/// Road classification for heuristic edge attribute derivation.
///
/// Mirrors velos-net RoadClass without creating a dependency from core to net.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RoadClass {
    Motorway,
    Trunk,
    Primary,
    Secondary,
    Tertiary,
    Residential,
    Service,
}

/// Derive default edge attributes from road class heuristics.
///
/// Used when no ground-truth safety/comfort data is available (HCMC POC).
/// Values are configurable starting points based on road class characteristics.
pub fn default_edge_attributes(
    road_class: RoadClass,
    length_m: f32,
    speed_limit_kmh: f32,
    has_signal: bool,
) -> EdgeAttributes {
    let speed_ms = speed_limit_kmh / 3.6;
    let current_travel_time = if speed_ms > 0.0 {
        length_m / speed_ms
    } else {
        f32::MAX
    };

    let (comfort_penalty, safety_score) = match road_class {
        RoadClass::Motorway => (0.8, 0.2),
        RoadClass::Trunk => (0.8, 0.2),
        RoadClass::Primary => (0.4, 0.4),
        RoadClass::Secondary => (0.2, 0.5),
        RoadClass::Tertiary => (0.2, 0.5),
        RoadClass::Residential => (0.1, 0.6),
        RoadClass::Service => (0.3, 0.8),
    };

    let fuel_rate = 0.08 / 1000.0; // 0.08 L/km = 0.00008 L/m

    let signal_delay = if has_signal { 15.0 } else { 0.0 };

    EdgeAttributes {
        current_travel_time,
        distance_m: length_m,
        comfort_penalty,
        safety_score,
        fuel_rate,
        signal_delay,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commuter_has_time_as_highest_weight() {
        let w = &PROFILE_WEIGHTS[AgentProfile::Commuter as usize];
        assert!(w.time > w.comfort);
        assert!(w.time > w.safety);
        assert!(w.time > w.fuel);
        assert!(w.time > w.signal_delay);
        assert!(w.time > w.prediction_penalty);
    }

    #[test]
    fn tourist_has_comfort_and_safety_as_top_two() {
        let w = &PROFILE_WEIGHTS[AgentProfile::Tourist as usize];
        let mut factors = [
            ("time", w.time),
            ("comfort", w.comfort),
            ("safety", w.safety),
            ("fuel", w.fuel),
            ("signal_delay", w.signal_delay),
            ("prediction_penalty", w.prediction_penalty),
        ];
        factors.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        assert_eq!(factors[0].0, "comfort");
        assert_eq!(factors[1].0, "safety");
    }

    #[test]
    fn emergency_has_time_080() {
        let w = &PROFILE_WEIGHTS[AgentProfile::Emergency as usize];
        assert!((w.time - 0.80).abs() < f32::EPSILON);
    }

    #[test]
    fn all_profiles_weights_sum_to_one() {
        for (i, w) in PROFILE_WEIGHTS.iter().enumerate() {
            let sum = w.sum();
            assert!(
                (sum - 1.0).abs() < 1e-5,
                "Profile {} weights sum to {} (expected 1.0)",
                i,
                sum
            );
        }
    }

    #[test]
    fn route_cost_time_only_returns_sum_of_travel_times() {
        let weights = CostWeights {
            time: 1.0,
            comfort: 0.0,
            safety: 0.0,
            fuel: 0.0,
            signal_delay: 0.0,
            prediction_penalty: 0.0,
        };
        // Two edges, both nearby (within 2km blend distance)
        let edges = vec![0u32, 1];
        let attrs = vec![
            EdgeAttributes {
                current_travel_time: 10.0,
                distance_m: 500.0,
                comfort_penalty: 0.5,
                safety_score: 0.3,
                fuel_rate: 0.0001,
                signal_delay: 5.0,
            },
            EdgeAttributes {
                current_travel_time: 20.0,
                distance_m: 500.0,
                comfort_penalty: 0.5,
                safety_score: 0.3,
                fuel_rate: 0.0001,
                signal_delay: 5.0,
            },
        ];
        let overlay_times = vec![10.0_f32, 20.0];
        let overlay_conf = vec![1.0_f32, 1.0];

        let cost = route_cost(&edges, &overlay_times, &overlay_conf, &weights, &attrs);
        // Nearby edges (cum_dist < 2000m), so blend=0 for first, blend=500/2000=0.25 for second
        // First: travel_time = 10*(1-0) + 10*0 = 10
        // Second: travel_time = 20*(1-0.25) + 20*0.25 = 20
        // Total = 10 + 20 = 30
        assert!((cost - 30.0).abs() < 1e-3, "Got {cost}");
    }

    #[test]
    fn route_cost_different_weights_produce_different_costs() {
        let edges = vec![0u32];
        let attrs = vec![EdgeAttributes {
            current_travel_time: 10.0,
            distance_m: 1000.0,
            comfort_penalty: 0.5,
            safety_score: 0.3,
            fuel_rate: 0.0001,
            signal_delay: 5.0,
        }];
        let overlay_times = vec![10.0_f32];
        let overlay_conf = vec![1.0_f32];

        let commuter_cost = route_cost(
            &edges,
            &overlay_times,
            &overlay_conf,
            &PROFILE_WEIGHTS[AgentProfile::Commuter as usize],
            &attrs,
        );
        let tourist_cost = route_cost(
            &edges,
            &overlay_times,
            &overlay_conf,
            &PROFILE_WEIGHTS[AgentProfile::Tourist as usize],
            &attrs,
        );
        assert!(
            (commuter_cost - tourist_cost).abs() > 0.01,
            "Commuter={commuter_cost} Tourist={tourist_cost} should differ"
        );
    }

    #[test]
    fn route_cost_low_confidence_applies_prediction_penalty() {
        let weights = CostWeights {
            time: 0.5,
            comfort: 0.0,
            safety: 0.0,
            fuel: 0.0,
            signal_delay: 0.0,
            prediction_penalty: 0.5,
        };
        let edges = vec![0u32];
        let attrs = vec![EdgeAttributes {
            current_travel_time: 10.0,
            distance_m: 100.0,
            comfort_penalty: 0.0,
            safety_score: 0.0,
            fuel_rate: 0.0,
            signal_delay: 0.0,
        }];
        let overlay_times = vec![10.0_f32];

        // High confidence: no penalty
        let cost_high = route_cost(&edges, &overlay_times, &[0.9], &weights, &attrs);
        // Low confidence: penalty applied
        let cost_low = route_cost(&edges, &overlay_times, &[0.1], &weights, &attrs);

        assert!(
            cost_low > cost_high,
            "Low confidence ({cost_low}) should cost more than high confidence ({cost_high})"
        );
    }

    #[test]
    fn route_cost_distance_weighted_blend() {
        let weights = CostWeights {
            time: 1.0,
            comfort: 0.0,
            safety: 0.0,
            fuel: 0.0,
            signal_delay: 0.0,
            prediction_penalty: 0.0,
        };
        // First edge: 100m (nearby, uses observed=10s)
        // Second edge: 3000m (far, cumulative=100+3000=3100m, heavily predicted)
        let edges = vec![0u32, 1];
        let attrs = vec![
            EdgeAttributes {
                current_travel_time: 10.0,
                distance_m: 100.0,
                comfort_penalty: 0.0,
                safety_score: 0.0,
                fuel_rate: 0.0,
                signal_delay: 0.0,
            },
            EdgeAttributes {
                current_travel_time: 10.0,
                distance_m: 3000.0,
                comfort_penalty: 0.0,
                safety_score: 0.0,
                fuel_rate: 0.0,
                signal_delay: 0.0,
            },
        ];
        // Predicted time is very different from observed for second edge
        let overlay_times = vec![10.0_f32, 50.0];
        let overlay_conf = vec![1.0_f32, 1.0];

        let cost = route_cost(&edges, &overlay_times, &overlay_conf, &weights, &attrs);
        // First edge: blend=0/2000=0, travel_time=10*(1-0)+10*0=10
        // Second edge: blend=min(100/2000,1)=0.05, travel_time=10*(1-0.05)+50*0.05=9.5+2.5=12
        // Total = 10 + 12 = 22
        assert!((cost - 22.0).abs() < 0.1, "Got {cost}, expected ~22.0");
    }

    #[test]
    fn encode_profile_preserves_low_bits() {
        // Set bits 0-2 (at_bus_stop=1, emergency_active=1, yielding=1) = 0b0111 = 7
        let flags = 0x07;
        let encoded = encode_profile_in_flags(flags, AgentProfile::Tourist);
        // Tourist = 4, shifted left 4 = 0x40
        assert_eq!(encoded & 0x0F, 0x07, "Low bits should be preserved");
        assert_eq!((encoded >> 4) & 0x0F, 4, "Profile should be Tourist (4)");
    }

    #[test]
    fn decode_profile_returns_correct_id() {
        for profile in [
            AgentProfile::Commuter,
            AgentProfile::Bus,
            AgentProfile::Truck,
            AgentProfile::Emergency,
            AgentProfile::Tourist,
            AgentProfile::Teen,
            AgentProfile::Senior,
            AgentProfile::Cyclist,
        ] {
            let flags = encode_profile_in_flags(0, profile);
            let decoded = decode_profile_from_flags(flags);
            assert_eq!(decoded, profile, "Round-trip failed for {profile:?}");
        }
    }

    #[test]
    fn encode_decode_roundtrip_with_existing_flags() {
        let original_flags = 0x05; // bits 0 and 2 set
        let profile = AgentProfile::Senior;
        let encoded = encode_profile_in_flags(original_flags, profile);
        let decoded = decode_profile_from_flags(encoded);
        assert_eq!(decoded, profile);
        assert_eq!(encoded & 0x0F, original_flags);
    }

    #[test]
    fn default_edge_attributes_motorway_lower_comfort_than_residential() {
        let motorway = default_edge_attributes(RoadClass::Motorway, 1000.0, 100.0, false);
        let residential = default_edge_attributes(RoadClass::Residential, 1000.0, 30.0, false);
        assert!(
            motorway.comfort_penalty > residential.comfort_penalty,
            "Motorway comfort_penalty ({}) should be higher (less comfortable) than Residential ({})",
            motorway.comfort_penalty,
            residential.comfort_penalty
        );
    }

    #[test]
    fn default_edge_attributes_service_higher_safety_score_than_motorway() {
        let service = default_edge_attributes(RoadClass::Service, 500.0, 20.0, false);
        let motorway = default_edge_attributes(RoadClass::Motorway, 500.0, 100.0, false);
        assert!(
            service.safety_score > motorway.safety_score,
            "Service safety_score ({}) should be higher (less safe) than Motorway ({})",
            service.safety_score,
            motorway.safety_score
        );
    }

    #[test]
    fn default_edge_attributes_signal_delay() {
        let with_signal = default_edge_attributes(RoadClass::Primary, 500.0, 50.0, true);
        let no_signal = default_edge_attributes(RoadClass::Primary, 500.0, 50.0, false);
        assert!((with_signal.signal_delay - 15.0).abs() < f32::EPSILON);
        assert!((no_signal.signal_delay - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn default_edge_attributes_travel_time_calculation() {
        let attr = default_edge_attributes(RoadClass::Primary, 1000.0, 36.0, false);
        // 36 km/h = 10 m/s, 1000m / 10 m/s = 100s
        assert!((attr.current_travel_time - 100.0).abs() < 0.1);
    }
}
