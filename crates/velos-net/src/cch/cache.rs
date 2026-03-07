//! Binary disk cache for CCH ordering and topology.
//!
//! Uses postcard for compact serialization, matching velos-net's graph caching pattern.

use std::path::Path;

use crate::cch::CCHRouter;
use crate::error::NetError;

/// Save a CCHRouter to a binary file.
pub fn save_cch(router: &CCHRouter, path: &Path) -> Result<(), NetError> {
    let bytes = postcard::to_allocvec(router)
        .map_err(|e| NetError::Serialization(format!("CCH postcard serialize: {e}")))?;
    std::fs::write(path, bytes)?;
    Ok(())
}

/// Load a CCHRouter from a binary file.
pub fn load_cch(path: &Path) -> Result<CCHRouter, NetError> {
    let bytes = std::fs::read(path)?;
    let router: CCHRouter = postcard::from_bytes(&bytes)
        .map_err(|e| NetError::Serialization(format!("CCH postcard deserialize: {e}")))?;
    Ok(router)
}
