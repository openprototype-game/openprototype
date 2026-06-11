//! Prototype (1995) port: front-end shell.
//!
//! Opens the original disc image, loads the menu assets, and runs the menu in a
//! window. The game data is never bundled: point `--cue` at your own copy of
//! `PROTOTYPE.cue`, or drop it in the working directory (or set `$PROTOTYPE_DISC`)
//! and omit the flag. Built without the `desktop` feature there is no window
//! backend, so the binary just explains how to rebuild.

#[cfg(feature = "desktop")]
mod desktop {
    use std::path::PathBuf;
    use std::sync::Arc;

    use anyhow::{Context, Result};
    use clap::Parser;
    use openprototype::app::App;
    use openprototype::assets::{
        load_highscore_assets, load_intro_assets, load_level_assets, load_menu_assets,
    };
    use openprototype::highscores::HighscoreStore;
    use openprototype::levels::Level;
    use openprototype::scene::SceneId;
    use openprototype_backend::run;
    use prototype_disc::{DiscImage, manifest};
    use tracing_subscriber::EnvFilter;

    /// Which scene to boot straight into, bypassing the normal intro flow.
    #[derive(Clone, Copy, clap::ValueEnum)]
    enum DevScene {
        /// The in-game level render (weapon-animation test harness).
        Level,
    }

    #[derive(Parser)]
    #[command(about = "Prototype (1995) front-end")]
    struct Cli {
        /// Path to the disc image cue sheet. Defaults to `$PROTOTYPE_DISC`, or
        /// `./PROTOTYPE.cue` in the current directory if that is unset.
        #[arg(long)]
        cue: Option<PathBuf>,

        /// Boot straight into a developer scene instead of the intro.
        #[arg(long)]
        scene: Option<DevScene>,

        /// Which level to load for the `--scene level` harness (1..=7).
        #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u8).range(1..=7))]
        level: u8,

        /// Skip this many seconds into the level (`--scene level` only): the
        /// spawn clock, enemies, and scroll are pre-simulated, so the scene
        /// starts mid-action.
        #[arg(long, default_value_t = 0.0)]
        skip: f32,
    }

    /// Our crates at `info`, everything else (wgpu, winit, rodio) at `warn`.
    /// `RUST_LOG` overrides this entirely when set.
    const DEFAULT_LOG: &str = "warn,openprototype=info,openprototype_backend=info,\
        openprototype_core=info,prototype_disc=info,prototype_formats=info";

    fn init_tracing() {
        let filter =
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(DEFAULT_LOG));

        tracing_subscriber::fmt().with_env_filter(filter).init();
    }

    /// Check the data track against the build manifest before touching any
    /// baked-in offsets, so a wrong or corrupted image fails with a per-file
    /// report instead of a decoder error.
    fn verify_disc(disc: &DiscImage) -> Result<()> {
        let mismatches = manifest::verify(disc).context("verifying the disc image")?;

        if mismatches.is_empty() {
            return Ok(());
        }

        let report = mismatches
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n  ");
        anyhow::bail!(
            "the disc image does not match the build this port supports:\n  {report}\n\
             The port bakes in byte offsets for one specific pressing; a different \
             pressing or a damaged rip will not work."
        );
    }

    pub fn main() -> Result<()> {
        init_tracing();

        let cli = Cli::parse();

        let disc = Arc::new(match &cli.cue {
            Some(cue) => DiscImage::open(cue)
                .with_context(|| format!("opening disc image {}", cue.display()))?,
            None => DiscImage::open_default()
                .context("opening the default disc image (set --cue or $PROTOTYPE_DISC)")?,
        });

        verify_disc(&disc)?;

        let menu_assets = load_menu_assets(&disc)?;
        let intro_assets = load_intro_assets(&disc)?;
        let highscore_assets = load_highscore_assets(&disc)?;
        let level = Level::from_number(cli.level).expect("--level is validated to 1..=7");
        let level_assets = load_level_assets(&disc, level)?;
        let highscore_store = HighscoreStore::open(&disc)?;
        let mut app = App::new(
            menu_assets,
            intro_assets,
            highscore_assets,
            level_assets,
            highscore_store,
        );

        if let Some(DevScene::Level) = cli.scene {
            app.set_level_skip((cli.skip * 60.0) as u32);
            app.start_on(SceneId::Level);
        }

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
