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
//! - `bookmarks`: client-side bookmark page
//! - `settings`:  user settings (profile, appearance, password)
//! - `admin`:     admin dashboard + branding (site title, theme, legal)
//! - `admin_email`: SMTP settings, unverified sign-up switch, test send
//! - `legal`:     public terms + privacy pages
//! - `setup`:     first-run admin claim and the setup gate middleware
//! - `signup`:    open registration verified by an emailed link
pub mod admin;
pub mod admin_email;
pub mod api;
pub mod auth_pages;
pub mod bookmarks;
pub mod check;
pub mod health;
pub mod home;
pub mod legal;
pub mod manga;
pub mod notes;
pub mod oauth;
pub mod password_reset;
pub mod reader;
pub mod reader_prefs;
pub mod series;
pub mod settings;
pub mod setup;
pub mod signup;
pub mod source;
pub mod sync;
