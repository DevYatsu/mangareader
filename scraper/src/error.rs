use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Scraping failed: {0}")]
    Scrape(String),
    #[error("Database error: {0}")]
    Database(#[from] mangaread_core::error::Error),
    #[error("WebDriver error: {0}")]
    WebDriver(#[from] thirtyfour::error::WebDriverError),
}
