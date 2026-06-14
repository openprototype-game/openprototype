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
    use clap::{Parser, Subcommand};
    use openprototype::app::{App, FrontEndAssets};
    use openprototype::assets::{
        load_ending_assets, load_fli_bytes, load_gameover_assets, load_highscore_assets,
        load_intro_assets, load_level_assets, load_menu_assets,
    };
    use openprototype::highscores::HighscoreStore;
    use openprototype::levels::Level;
    use openprototype::savegame::SaveGame;
    use openprototype::scene::SceneId;
    use openprototype_backend::{WindowIcon, run};
    use openprototype_core::game_state::Handoff;
    use openprototype_install::{IconSource, InstallSpec};
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
        /// Path to the disc image cue sheet. Defaults to `$PROTOTYPE_DISC`, the
        /// path recorded by `install`, or `./PROTOTYPE.cue`.
        #[arg(long, global = true)]
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

        /// Boot straight into a `.psg` savegame (race levels only so far).
        #[arg(long, conflicts_with = "scene")]
        load: Option<PathBuf>,

        #[command(subcommand)]
        command: Option<Command>,
    }

    #[derive(Subcommand)]
    enum Command {
        /// Install OpenPrototype for the current user: copy the binary, place
        /// the disc in the data directory, and add a launcher entry with an
        /// icon decoded from the disc. Offline; pass `--cue` to point at the
        /// disc to install from.
        Install,
    }

    /// Our crates at `info`, everything else (wgpu, winit, rodio) at `warn`.
    ///
    /// `RUST_LOG` overrides this entirely when set.
    const DEFAULT_LOG: &str = "warn,openprototype=info,openprototype_backend=info,\
        openprototype_core=info,prototype_disc=info,prototype_formats=info";

    fn init_tracing() {
        let filter =
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(DEFAULT_LOG));

        tracing_subscriber::fmt().with_env_filter(filter).init();
    }

    /// Checks the data track against the build manifest before using any offsets.
    ///
    /// A wrong or corrupted image fails with a per-file report instead of a
    /// decoder error.
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

    /// The disc location of LEVEL_1's ship, for the window and launcher icons.
    fn icon_source() -> IconSource {
        let data = Level::L1.data();

        IconSource {
            wad_name: data.wad,
            ship_catalog: data.ship.catalog,
            palette_offset: data.palette_offset,
        }
    }

    /// The window icon: the player ship decoded from the disc.
    ///
    /// Shares the decoder with the launcher icon; a read/decode failure (an odd
    /// disc) just leaves the window without an icon.
    fn window_icon(disc: &DiscImage) -> Option<WindowIcon> {
        openprototype_install::decode_ship_icon(disc, &icon_source())
            .map(|icon| WindowIcon {
                rgba: icon.rgba,
                width: icon.side,
                height: icon.side,
            })
            .map_err(|error| tracing::warn!(%error, "no window icon"))
            .ok()
    }

    /// Resolves the disc cue path: `--cue`, then `$PROTOTYPE_DISC`, then the
    /// path recorded by `install`, then `./PROTOTYPE.cue`.
    fn resolve_cue(cli: &Cli) -> Option<PathBuf> {
        if let Some(cue) = &cli.cue {
            return Some(cue.clone());
        }

        if let Some(env) = std::env::var_os("PROTOTYPE_DISC") {
            return Some(PathBuf::from(env));
        }

        if let Some(recorded) = openprototype_install::configured_disc() {
            return Some(recorded);
        }

        let local = PathBuf::from("PROTOTYPE.cue");
        local.exists().then_some(local)
    }

    fn open_disc(cli: &Cli) -> Result<DiscImage> {
        let cue = resolve_cue(cli).context(
            "no disc image found: pass --cue, set $PROTOTYPE_DISC, run `install`, \
             or put PROTOTYPE.cue in the working directory",
        )?;

        DiscImage::open(&cue).with_context(|| format!("opening the disc image {}", cue.display()))
    }

    /// Runs the `install` subcommand: local, offline desktop integration.
    fn run_install(cli: &Cli) -> Result<()> {
        let cue = resolve_cue(cli)
            .context("no disc image to install from: pass --cue with the downloaded .cue")?;
        let report = openprototype_install::install(&InstallSpec {
            cue,
            icon: icon_source(),
        })?;

        println!("Installed OpenPrototype:");
        println!("  binary:   {}", report.binary.display());
        println!("  disc:     {}", report.disc.display());
        println!("  launcher: {}", report.launcher.display());
        println!("  icon:     {}", report.icon.display());

        Ok(())
    }

    pub fn main() -> Result<()> {
        init_tracing();

        let cli = Cli::parse();

        if let Some(Command::Install) = cli.command {
            return run_install(&cli);
        }

        let disc = Arc::new(open_disc(&cli)?);

        verify_disc(&disc)?;

        let assets = FrontEndAssets {
            menu: load_menu_assets(&disc)?,
            intro: load_intro_assets(&disc)?,
            highscore: load_highscore_assets(&disc)?,
            gameover: load_gameover_assets(&disc)?,
            ending: load_ending_assets(&disc)?,
        };
        let highscore_store = HighscoreStore::open(&disc)?;
        let loader_disc = disc.clone();
        let fli_disc = disc.clone();
        let mut app = App::new(
            assets,
            Box::new(move |level| load_level_assets(&loader_disc, level)),
            Box::new(move |name| load_fli_bytes(&fli_disc, name)),
            highscore_store,
        );

        if let Some(path) = &cli.load {
            let bytes = std::fs::read(path)
                .with_context(|| format!("reading savegame {}", path.display()))?;
            let save = SaveGame::decode(&bytes)
                .with_context(|| format!("decoding savegame {}", path.display()))?;
            app.start_on_save(save);
        }

        if let Some(DevScene::Level) = cli.scene {
            let level = Level::from_number(cli.level).expect("--level is validated to 1..=7");
            app.set_level_skip((cli.skip * 60.0) as u32);
            app.start_on(SceneId::Level {
                level,
                handoff: Handoff::new_game(),
            });
        }

        let icon = window_icon(&disc);

        run(Box::new(app), disc, icon)
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
