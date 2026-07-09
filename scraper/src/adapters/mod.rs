//! Concrete [`SiteAdapter`] implementations and shared scraping utilities.
//!
//! Each submodule implements the three-phase scraping contract for a specific
//! scanlation site. The [`helpers`] module provides common functions used by all
//! adapters (URL resolution, chapter number extraction, page-waiting).

pub mod demonicscans;
pub mod helpers;
pub mod manhuaplus;
