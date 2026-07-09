//! Domain types for manga, chapters, pages, series, and source-site scraping.
//!
//! Re-exported at the crate root.  Sub-modules mirror the entity graph:
//! `series` → `manga` → `chapter` → `page`.

pub mod chapter;
pub mod manga;
pub mod page;
pub mod series;
pub mod source;
pub mod title;

pub use chapter::*;
pub use manga::*;
pub use page::*;
pub use series::*;
pub use source::*;
pub use title::*;
