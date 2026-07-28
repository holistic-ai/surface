//! Why a section of the scan has no answer.
//!
//! Stable strings, because they are matched on rather than displayed raw. The
//! important one is that "could not read" is never the same as "nothing there":
//! a browser we lack permission for must not look like a browser with no AI
//! use in it.

/// Only the browser scan can hit this, so it is gated with it.
#[cfg(feature = "sqlite")]
pub const INSUFFICIENT_PRIVILEGES: &str = "insufficient_privileges";
pub const TOOL_UNAVAILABLE: &str = "tool_unavailable";
