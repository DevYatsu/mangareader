use clap::{Parser, Subcommand};
use mangaread_core::db;
use scraper::adapters::demonicscans::DemonicscansAdapter;
use scraper::engine::ScraperEngine;
use thirtyfour::prelude::*;
use tracing_subscriber;

#[derive(Parser)]
#[command(name = "manga-scraper")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Scan all adapters for manga listings
    Scan,
    /// Fetch details + chapters for a manga
    SyncManga {
        /// Manga ID in the database
        id: i64,
    },
    /// Fetch page images for a chapter
    SyncChapter {
        /// Chapter ID in the database
        id: i64,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    // Connect to WebDriver (expects chromedriver/geckodriver on localhost:4444)
    let caps = DesiredCapabilities::chrome();
    let driver = WebDriver::new("http://localhost:4444", caps).await?;

    // Open DB
    let conn = rusqlite::Connection::open("mangaread.db")?;
    db::init_db(&conn)?;

    let mut engine = ScraperEngine::new(conn);
    engine.add_adapter(Box::new(DemonicscansAdapter));

    match cli.command {
        Commands::Scan => {
            engine.scan_all(&driver).await?;
            let db = engine.into_db();
            let count: i64 = db
                .query_row("SELECT COUNT(*) FROM manga", [], |row| row.get(0))
                .unwrap_or(0);
            tracing::info!("scan complete. {} manga in database.", count);
        }
        Commands::SyncManga { id } => {
            engine.sync_manga(&driver, id).await?;
            tracing::info!("manga {} synced.", id);
        }
        Commands::SyncChapter { id } => {
            engine.sync_chapter(&driver, id).await?;
            tracing::info!("chapter {} synced.", id);
        }
    }

    driver.quit().await?;
    Ok(())
}
