//! Origin-Destination matrix for zone-to-zone trip volumes.

use std::collections::HashMap;

/// Traffic analysis zones for the District 1 POC area.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Zone {
    BenThanh,
    NguyenHue,
    Bitexco,
    BuiVien,
    Waterfront,
}

/// Origin-Destination matrix storing zone-to-zone trip counts (trips per hour).
#[derive(Debug, Clone)]
pub struct OdMatrix {
    trips: HashMap<(Zone, Zone), u32>,
}

impl OdMatrix {
    /// Create an empty OD matrix.
    pub fn new() -> Self {
        Self {
            trips: HashMap::new(),
        }
    }

    /// Set trips per hour for a zone pair.
    pub fn set_trips(&mut self, from: Zone, to: Zone, count: u32) {
        if count == 0 {
            self.trips.remove(&(from, to));
        } else {
            self.trips.insert((from, to), count);
        }
    }

    /// Get trips per hour for a zone pair. Returns 0 if not configured.
    pub fn get_trips(&self, from: Zone, to: Zone) -> u32 {
        self.trips.get(&(from, to)).copied().unwrap_or(0)
    }

    /// Sum of all trips across all zone pairs.
    pub fn total_trips(&self) -> u32 {
        self.trips.values().sum()
    }

    /// Iterate over all non-zero zone pairs: (from, to, count).
    pub fn zone_pairs(&self) -> impl Iterator<Item = (Zone, Zone, u32)> + '_ {
        self.trips
            .iter()
            .map(|(&(from, to), &count)| (from, to, count))
    }

    /// Factory: District 1 POC OD matrix with 5 zones and realistic trip volumes.
    ///
    /// Models major pedestrian and vehicle flows in HCMC District 1:
    /// - Ben Thanh Market hub (highest trip generation)
    /// - Nguyen Hue Walking Street (tourist/commercial)
    /// - Bitexco Financial Tower (office commuter)
    /// - Bui Vien (backpacker/nightlife area)
    /// - Waterfront (Saigon River promenade)
    ///
    /// Total: ~500 trips/hour across all OD pairs.
    pub fn district1_poc() -> Self {
        let mut od = Self::new();

        // Ben Thanh is the major hub -- highest trip generation
        od.set_trips(Zone::BenThanh, Zone::NguyenHue, 80);
        od.set_trips(Zone::NguyenHue, Zone::BenThanh, 75);

        od.set_trips(Zone::BenThanh, Zone::Bitexco, 70);
        od.set_trips(Zone::Bitexco, Zone::BenThanh, 65);

        od.set_trips(Zone::BuiVien, Zone::BenThanh, 55);
        od.set_trips(Zone::BenThanh, Zone::BuiVien, 50);

        // Waterfront corridor
        od.set_trips(Zone::BuiVien, Zone::Waterfront, 60);
        od.set_trips(Zone::Waterfront, Zone::BuiVien, 55);

        // Cross flows
        od.set_trips(Zone::NguyenHue, Zone::Waterfront, 50);

        od
    }
}

impl Default for OdMatrix {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_matrix_is_empty() {
        let od = OdMatrix::new();
        assert_eq!(od.total_trips(), 0);
    }

    #[test]
    fn set_zero_removes_pair() {
        let mut od = OdMatrix::new();
        od.set_trips(Zone::BenThanh, Zone::Bitexco, 50);
        od.set_trips(Zone::BenThanh, Zone::Bitexco, 0);
        assert_eq!(od.total_trips(), 0);
        assert_eq!(od.zone_pairs().count(), 0);
    }

    #[test]
    fn district1_poc_total_in_range() {
        let od = OdMatrix::district1_poc();
        let total = od.total_trips();
        // 80+75+70+65+55+50+60+55+50 = 560
        assert_eq!(total, 560);
    }
}
