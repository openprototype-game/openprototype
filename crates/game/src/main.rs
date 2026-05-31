//! Prototype (1995) port: front-end shell.
//!
//! Opens the original disc image, loads the menu assets, and runs the menu in a
//! window. The game data is never bundled: point `--cue` at your own copy of
//! `PROTOTYPE.cue`. Built without the `desktop` feature there is no window
//! backend, so the binary just explains how to rebuild.

#[cfg(feature = "desktop")]
mod desktop {
    use std::path::PathBuf;
    use std::sync::Arc;

    use anyhow::{Context, Result};
    use clap::Parser;
    use openprototype::app::App;
    use openprototype::assets::{load_highscore_assets, load_intro_assets, load_menu_assets};
    use openprototype::highscores::HighscoreStore;
    use openprototype::platform::run;
    use prototype_disc::DiscImage;

    #[derive(Parser)]
    #[command(about = "Prototype (1995) front-end")]
    struct Cli {
        /// Path to the disc image cue sheet (e.g. PROTOTYPE.cue).
        #[arg(long)]
        cue: PathBuf,
    }

    pub fn main() -> Result<()> {
        let cli = Cli::parse();

        let disc = Arc::new(
            DiscImage::open(&cli.cue)
                .with_context(|| format!("opening disc image {}", cli.cue.display()))?,
        );
        let menu_assets = load_menu_assets(&disc)?;
        let intro_assets = load_intro_assets(&disc)?;
        let highscore_assets = load_highscore_assets(&disc)?;
        let highscore_store = HighscoreStore::open(&disc)?;
        let app = App::new(menu_assets, intro_assets, highscore_assets, highscore_store);

        run(Box::new(app), disc)
    }
}

#[cfg(feature = "desktop")]
fn main() -> anyhow::Result<()> {
    desktop::main()
}

#[cfg(not(feature = "desktop"))]
fn main() {
    eprintln!(
        "Built without the `desktop` feature; rebuild with default features to run the window."
    );
}
