//! Feature-off stand-in for the VMCE scenario-catalog refresh.
//!
//! A build without the `vmce` capability never registers the mode row and
//! never scans `Maps/VMCE`; the catalog refresh is the only lobby seam left.

use crate::app::AppState;

pub(crate) fn refresh_vmce_entries(_state: &mut AppState) {}
