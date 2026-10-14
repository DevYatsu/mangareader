use clap::{Parser, Subcommand};
use mangaread_core::db;
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
    let driver = WebDriver::managed(DesiredCapabilities::chrome()).await?;

    // Open DB
    let conn = db::open_db()?;
    db::init_db(&conn)?;

    let engine = ScraperEngine::from_adapters(scraper::adapters::all());

    match cli.command {
        Commands::Scan => {
            for (source, result) in engine.scan_all(&driver, &conn).await {
                match result {
                    Ok(n) => tracing::info!("{source}: {n} listings"),
                    Err(e) => tracing::error!("{source}: scan failed: {e:#}"),
                }
            }
            let count: i64 = conn
                .query_row("SELECT COUNT(*) FROM manga", [], |row| row.get(0))
                .unwrap_or(0);
            tracing::info!("scan complete. {} manga in database.", count);
        }
        Commands::SyncManga { id } => {
            let n = engine.sync_manga(&driver, &conn, id).await?;
            tracing::info!("manga {} synced ({n} chapters).", id);
        }
        Commands::SyncChapter { id } => {
            let n = engine.sync_chapter(&driver, &conn, id).await?;
            tracing::info!("chapter {} synced ({n} pages).", id);
        }
    }

    driver.quit().await?;
    Ok(())
}
