#[cfg(feature = "rsraw")]
pub mod preview_rsraw;
#[cfg(not(feature = "rsraw"))]
pub mod preview_heuristic;
pub mod files;
pub mod thumbnail;


#[cfg(feature = "rsraw")]
pub use preview_rsraw as preview;
#[cfg(not(feature = "rsraw"))]
pub use preview_heuristic as preview;