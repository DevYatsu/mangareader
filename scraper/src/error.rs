use thiserror::Error;

/// Errors that can occur during scraping operations.
///
/// Wraps scrape-specific failures, database errors (delegated to the core crate), and
/// Selenium WebDriver errors.
#[derive(Error, Debug)]
pub enum Error {
    /// A scraping operation failed with a descriptive message.
    ///
    /// Raised when HTML parsing, selector matching, or data extraction produces an
    /// unexpected or missing value.
    #[error("Scraping failed: {0}")]
    Scrape(String),

    /// A database operation failed.
    ///
    /// Automatically converted from [`mangaread_core::error::Error`] via `#[from]`.
    #[error("Database error: {0}")]
    Database(#[from] mangaread_core::error::Error),

    /// The Selenium WebDriver encountered an error (timeout, navigation failure, etc.).
    ///
    /// Automatically converted from [`thirtyfour::error::WebDriverError`] via `#[from]`.
    #[error("WebDriver error: {0}")]
    WebDriver(#[from] thirtyfour::error::WebDriverError),
}
