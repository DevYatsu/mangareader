//! Concrete [`SiteAdapter`] implementations and shared scraping utilities.
//!
//! Each submodule implements the three-phase scraping contract for a specific
//! scanlation site. The [`helpers`] module provides common functions used by all
//! adapters (URL resolution, chapter number extraction, page-waiting).

pub mod demonicscans;
pub mod helpers;
pub mod manhuaplus;

use crate::adapter::SiteAdapter;

/// Every built-in source, in scan order. This is the only registry: the
/// server, the CLI, and the admin source list all read it, so adding a
/// source is one line here.
pub fn all() -> Vec<Box<dyn SiteAdapter>> {
    vec![
        Box::new(demonicscans::DemonicscansAdapter),
        Box::new(manhuaplus::ManhuaplusAdapter),
    ]
}
