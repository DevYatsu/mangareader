//! Scraper — fetch manga metadata, chapter listings, and page images from third-party scanlation sites.
//!
//! Two build targets:
//! - **Library** (`lib`) — provides the [`SiteAdapter`] trait, adapter implementations, and the
//!   [`ScraperEngine`] orchestrator for use by other crates (e.g. the CLI binary).
//! - **Binary** (`bin`) — a standalone CLI that drives the engine via Selenium WebDriver.

pub mod adapter;
pub mod adapters;
pub mod engine;
pub mod error;
