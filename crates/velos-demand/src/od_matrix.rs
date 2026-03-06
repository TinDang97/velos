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
        todo!("implement new")
    }

    /// Set trips per hour for a zone pair.
    pub fn set_trips(&mut self, _from: Zone, _to: Zone, _count: u32) {
        todo!("implement set_trips")
    }

    /// Get trips per hour for a zone pair. Returns 0 if not configured.
    pub fn get_trips(&self, _from: Zone, _to: Zone) -> u32 {
        todo!("implement get_trips")
    }

    /// Sum of all trips across all zone pairs.
    pub fn total_trips(&self) -> u32 {
        todo!("implement total_trips")
    }

    /// Iterate over all non-zero zone pairs: (from, to, count).
    pub fn zone_pairs(&self) -> impl Iterator<Item = (Zone, Zone, u32)> + '_ {
        self.trips.iter().map(|(&(from, to), &count)| (from, to, count))
    }

    /// Factory: District 1 POC OD matrix with 5 zones and realistic trip volumes.
    pub fn district1_poc() -> Self {
        todo!("implement district1_poc")
    }
}

impl Default for OdMatrix {
    fn default() -> Self {
        Self::new()
    }
}
