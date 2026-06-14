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
    use prototype_disc::{AssetSource, DiscImage, manifest};
    use prototype_formats::bin::decode_ship;
    use prototype_formats::{Palette, Sprite, wad};
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

        /// Boot straight into a `.psg` savegame (race levels only so far).
        #[arg(long, conflicts_with = "scene")]
        load: Option<PathBuf>,
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

    /// Builds the window icon from the player ship, decoded from the disc.
    ///
    /// Bundles no art: the pixels come from `PTURN1.BN1` over LEVEL_1's palette,
    /// the same files the game plays from. A read or decode failure (an odd
    /// disc) just leaves the window without an icon.
    fn window_icon(disc: &DiscImage) -> Option<WindowIcon> {
        build_window_icon(disc)
            .map_err(|error| tracing::warn!(%error, "no window icon"))
            .ok()
    }

    fn build_window_icon(disc: &DiscImage) -> Result<WindowIcon> {
        let data = Level::L1.data();
        let wad = disc
            .read(data.wad)
            .with_context(|| format!("reading {}", data.wad))?;
        let pturn1 = disc.read("PTURN1.BN1").context("reading PTURN1.BN1")?;
        let frames =
            decode_ship(&pturn1, &wad, data.ship.catalog).context("decoding PTURN1.BN1")?;
        let palette =
            wad::palette_at(&wad, data.palette_offset).context("reading the level palette")?;
        let sprite = frames
            .sprites
            .first()
            .context("PTURN1.BN1 has no ship frames")?;

        Ok(ship_icon(sprite, &palette))
    }

    /// Composites one ship frame into a transparent square RGBA icon.
    ///
    /// Nearest-upscaled to the level's 1.5 pixel aspect (a 2:3 integer scale, so
    /// it stays crisp) and centered, so the desktop compositor downscales a large
    /// clean source instead of smoothing the ~37-pixel sprite up.
    fn ship_icon(sprite: &Sprite, palette: &Palette) -> WindowIcon {
        const SCALE_X: u32 = 8;
        const SCALE_Y: u32 = 12;
        const MARGIN: u32 = 18;

        let (sprite_w, sprite_h) = (sprite.size.width, sprite.size.height);
        let (scaled_w, scaled_h) = (sprite_w * SCALE_X, sprite_h * SCALE_Y);
        let side = scaled_w.max(scaled_h) + MARGIN * 2;
        let (origin_x, origin_y) = ((side - scaled_w) / 2, (side - scaled_h) / 2);

        let mut rgba = vec![0u8; (side * side * 4) as usize];

        for y in 0..sprite_h {
            for x in 0..sprite_w {
                let Some(index) = sprite.pixels[(y * sprite_w + x) as usize] else {
                    continue;
                };

                let color = palette.colors[usize::from(index)];

                for block_y in 0..SCALE_Y {
                    for block_x in 0..SCALE_X {
                        let px = origin_x + x * SCALE_X + block_x;
                        let py = origin_y + y * SCALE_Y + block_y;
                        let at = ((py * side + px) * 4) as usize;
                        rgba[at] = color.r;
                        rgba[at + 1] = color.g;
                        rgba[at + 2] = color.b;
                        rgba[at + 3] = 0xff;
                    }
                }
            }
        }

        WindowIcon {
            rgba,
            width: side,
            height: side,
        }
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
