//! Route handlers for the MangaRead web application.
//!
//! Each submodule handles a specific group of endpoints:
//! - `home`:    landing page with stats and recent chapters
//! - `series`:  series listing and detail pages
//! - `manga`:   manga-to-series redirect logic
//! - `reader`:  chapter reading interface
//! - `source`:  per-source manga listing
//! - `check`:   lightweight API endpoints for sync polling
//! - `health`:   liveness probe for Docker
//! - `api`:     sync management, progress tracking
pub mod api;
pub mod check;
pub mod health;
pub mod home;
pub mod manga;
pub mod reader;
pub mod series;
pub mod source;
