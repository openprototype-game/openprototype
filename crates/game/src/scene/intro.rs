//! The intro sequence.
//!
//! Mirrors `START.EXE`'s title script (`0x47db..0x4a68`): the NEO Software
//! still fades in over the starting title theme, then `intro.fli` plays, then
//! the publisher still, then `fly.fli`, and finally the credits (`credz.fli`
//! under the dev-team text pages) before the menu. The original's beat
//! durations are in 70 Hz ticks; they live here as data. Any key skips the
//! whole intro straight to the menu, as the original lets a keypress abort each
//! primitive.
//!
//! The scene drives one primitive at a time off the elapsed time and reports
//! [`is_animating`](Scene::is_animating), so the platform pumps it on the
//! retrace-rate timer. The FLIs are decoded from their bytes when their beat
//! starts; a (validated-at-load, so unexpected) decode failure skips the beat.

use std::rc::Rc;
use std::time::Duration;

use prototype_formats::font::Font;
use prototype_formats::{Dimensions, IndexedImage, Palette, Rgb};

use crate::assets::{IntroAssets, MenuAssets};
use crate::core::audio::AudioCommand;
use crate::core::fade::PaletteFade;
use crate::core::flic_player::FlicPlayer;
use crate::core::framebuffer::{Framebuffer, SCREEN_HEIGHT, SCREEN_WIDTH};
use crate::core::input::KeyEvent;
use crate::scene::{Menu, Scene, SceneId, SceneOutput, Transition};

/// The CD-DA track the intro starts (the title theme), kept playing into the
/// menu by the platform.
const TITLE_TRACK: u8 = 2;

/// One tick is 1/70 s, the original's VGA retrace rate and the unit of every
/// hold in its intro script.
const fn ticks(count: u64) -> Duration {
    Duration::from_micros(count * 1_000_000 / 70)
}


/// Which decoded still a [`Beat::Show`] puts on screen.
#[derive(Clone, Copy)]
enum Still {
    Neo,
    Surplogo,
    Cover,
}

/// Which FLI a [`Beat::Flic`] plays.
#[derive(Clone, Copy)]
enum Fli {
    Intro,
    Fly,
}

impl Fli {
    /// Per-frame delay the original forces (`cs:[0x3022]` before each play),
    /// ignoring the FLI's own header speed.
    fn frame_delay(self) -> Duration {
        match self {
            Fli::Intro => ticks(3),
            Fli::Fly => ticks(2),
        }
    }
}

/// Per-frame delay for `credz.fli` in the credits (`cs:[0x3022]=8`).
const CREDITS_FLI_DELAY: Duration = ticks(8);

/// One step of the intro script. `Show` and `ShowLit` are instant (they swap in
/// a still, behind black or at full palette respectively); the rest run over
/// time.
enum Beat {
    Show(Still),
    ShowLit(Still),
    FadeIn(Duration),
    Hold(Duration),
    FadeOut(Duration),
    Flic(Fli),
    Credits,
    /// Compose the real main-menu frame and fade it in from black, then end the
    /// intro. The original draws the menu under a black palette and fades it in
    /// (`0x4aff`) before entering the menu loop.
    FadeInMenu(Duration),
}

/// The primitive currently running.
enum Active {
    Fade(Box<PaletteFade>),
    Hold(Duration),
    Flic(FlicPlayer),
    Credits(Credits),
    Done,
}

pub struct Intro {
    assets: Rc<IntroAssets>,
    menu_assets: Rc<MenuAssets>,
    script: Vec<Beat>,
    index: usize,
    active: Active,
    /// The still currently behind the palette (for `FadeIn`/`Hold`/`FadeOut`).
    image: IndexedImage,
    /// The palette shown right now.
    displayed: Palette,
    /// Where `FadeIn` fades to: the active still's real palette.
    target: Palette,
    framebuffer: Framebuffer,
    started: bool,
}

impl Intro {
    pub fn new(assets: Rc<IntroAssets>, menu_assets: Rc<MenuAssets>) -> Self {
        // Durations are the original's, in 70 Hz ticks (START.EXE 0x47db..).
        // The fade primitive runs `steps * cs:[0x3022]` ticks, so the fades
        // differ per beat. The opening black hold is where CD track 2 starts.
        let script = vec![
            Beat::Show(Still::Neo),
            Beat::Hold(ticks(220)),
            Beat::FadeIn(ticks(220)),
            Beat::Hold(ticks(350)),
            Beat::Flic(Fli::Intro),
            Beat::Hold(ticks(230)),
            Beat::FadeOut(ticks(220)),
            Beat::Show(Still::Surplogo),
            Beat::FadeIn(ticks(110)),
            Beat::Hold(ticks(200)),
            Beat::FadeOut(ticks(110)),
            Beat::Flic(Fli::Fly),
            Beat::ShowLit(Still::Cover),
            Beat::Hold(ticks(180)),
            Beat::FadeOut(ticks(90)),
            Beat::Credits,
            Beat::FadeInMenu(ticks(40)),
        ];

        let mut intro = Self {
            framebuffer: Framebuffer::new(black()),
            assets,
            menu_assets,
            script,
            index: 0,
            active: Active::Done,
            image: blank_image(),
            displayed: black(),
            target: black(),
            started: false,
        };

        intro.start_from(0);
        intro.render();
        intro
    }

    /// Start (or skip past) the beat at `from`, resolving instant `Show` beats
    /// until a timed primitive or the end is reached.
    fn start_from(&mut self, from: usize) {
        let mut index = from;

        loop {
            let Some(beat) = self.script.get(index) else {
                self.active = Active::Done;
                return;
            };

            self.index = index;

            match beat {
                Beat::Show(still) => {
                    let still = self.still(*still);
                    let (image, palette) = (still.image.clone(), still.palette.clone());
                    self.image = image;
                    self.target = palette;
                    self.displayed = black();
                    index += 1;
                }
                Beat::ShowLit(still) => {
                    let still = self.still(*still);
                    let (image, palette) = (still.image.clone(), still.palette.clone());
                    self.image = image;
                    self.displayed = palette.clone();
                    self.target = palette;
                    index += 1;
                }
                Beat::FadeIn(duration) => {
                    self.active = Active::Fade(Box::new(PaletteFade::new(
                        black(),
                        self.target.clone(),
                        *duration,
                    )));
                    return;
                }
                Beat::Hold(duration) => {
                    self.active = Active::Hold(*duration);
                    return;
                }
                Beat::FadeOut(duration) => {
                    self.active = Active::Fade(Box::new(PaletteFade::new(
                        self.displayed.clone(),
                        black(),
                        *duration,
                    )));
                    return;
                }
                Beat::Flic(fli) => {
                    match FlicPlayer::decode_at(self.fli_bytes(*fli), fli.frame_delay()) {
                        Ok(player) => {
                            self.active = Active::Flic(player);
                            return;
                        }
                        Err(_) => index += 1,
                    }
                }
                Beat::Credits => match FlicPlayer::decode_at(
                    &self.assets.credz_fli,
                    CREDITS_FLI_DELAY,
                ) {
                    Ok(player) => {
                        self.active = Active::Credits(Credits::new(player));
                        return;
                    }
                    Err(_) => {
                        self.active = Active::Done;
                        return;
                    }
                },
                Beat::FadeInMenu(duration) => {
                    // Compose the real menu frame (background + labels + cursor)
                    // and fade its palette up from black. Building the same
                    // scene the app switches to keeps the handoff seamless.
                    let (image, palette) = {
                        let mut menu = Menu::new(self.menu_assets.clone());
                        let frame = menu.frame_without_cursor();
                        (frame.image.clone(), frame.palette.clone())
                    };
                    self.image = image;
                    self.target = palette.clone();
                    self.displayed = black();
                    self.active =
                        Active::Fade(Box::new(PaletteFade::new(black(), palette, *duration)));
                    return;
                }
            }
        }
    }

    fn still(&self, still: Still) -> &crate::assets::StillImage {
        match still {
            Still::Neo => &self.assets.neo,
            Still::Surplogo => &self.assets.surplogo,
            Still::Cover => &self.assets.cover,
        }
    }

    fn fli_bytes(&self, fli: Fli) -> &[u8] {
        match fli {
            Fli::Intro => &self.assets.intro_fli,
            Fli::Fly => &self.assets.fly_fli,
        }
    }

    /// Advance the running primitive; move to the next beat when it finishes.
    fn advance(&mut self, dt: Duration) {
        match &mut self.active {
            Active::Done => {}
            Active::Hold(remaining) => {
                *remaining = remaining.saturating_sub(dt);

                if remaining.is_zero() {
                    self.start_from(self.index + 1);
                }
            }
            Active::Fade(fade) => {
                fade.advance(dt);
                self.displayed = fade.current();

                if fade.finished() {
                    self.start_from(self.index + 1);
                }
            }
            Active::Flic(player) => {
                player.advance(dt);

                if player.finished() {
                    let frame = player.current();
                    self.image = frame.image.clone();
                    self.displayed = frame.palette.clone();
                    self.start_from(self.index + 1);
                }
            }
            Active::Credits(credits) => {
                credits.advance(dt);

                if credits.finished() {
                    self.start_from(self.index + 1);
                }
            }
        }
    }

    fn finished(&self) -> bool {
        matches!(self.active, Active::Done)
    }

    fn render(&mut self) {
        match &self.active {
            Active::Flic(player) => {
                let frame = player.current();
                self.framebuffer.blit_screen(&frame.image);
                self.framebuffer.palette = frame.palette.clone();
            }
            Active::Credits(credits) => credits.render(&mut self.framebuffer, &self.assets.font),
            _ => {
                self.framebuffer.blit_screen(&self.image);
                self.framebuffer.palette = self.displayed.clone();
            }
        }
    }
}

impl Scene for Intro {
    fn update(&mut self, dt: Duration, input: &[KeyEvent]) -> SceneOutput {
        let mut output = SceneOutput::default();

        if !self.started {
            output.audio.push(AudioCommand::PlayTrack(TITLE_TRACK));
            self.started = true;
        }

        if !input.is_empty() {
            output.transition = Some(Transition::To(SceneId::MainMenu));
            return output;
        }

        self.advance(dt);

        if self.finished() {
            output.transition = Some(Transition::To(SceneId::MainMenu));
            return output;
        }

        self.render();
        output
    }

    fn framebuffer(&self) -> &Framebuffer {
        &self.framebuffer
    }

    fn is_animating(&self) -> bool {
        true
    }
}

/// The credits: `credz.fli` loops throughout, one full play per "page".
/// Mirrors `START.EXE`'s credits routine (`0x460b`): it plays the FLI once with
/// no text (the lead-in), then plays it again under each text page in turn, and
/// the original's trailing blank page is one more text-free play (the linger)
/// before the menu. So the page on screen is keyed to the loop count.
struct Credits {
    player: FlicPlayer,
    rotation: usize,
}

impl Credits {
    /// Loops: 0 = lead-in, 1..=N = the pages, N+1 = the blank linger.
    const TOTAL_ROTATIONS: usize = CREDIT_PAGES.len() + 2;

    fn new(player: FlicPlayer) -> Self {
        Self {
            player,
            rotation: 0,
        }
    }

    fn advance(&mut self, dt: Duration) {
        self.player.advance(dt);

        if self.player.finished() {
            self.player.restart();
            self.rotation += 1;
        }
    }

    fn finished(&self) -> bool {
        self.rotation >= Self::TOTAL_ROTATIONS
    }

    fn render(&self, framebuffer: &mut Framebuffer, font: &Font) {
        let frame = self.player.current();
        framebuffer.blit_screen(&frame.image);
        framebuffer.palette = frame.palette.clone();

        // Pages run during loops 1..=N; the lead-in and the trailing linger
        // show the animation with no text.
        if (1..=CREDIT_PAGES.len()).contains(&self.rotation) {
            draw_centered(framebuffer, font, CREDIT_PAGES[self.rotation - 1]);
        }
    }
}

/// The dev-team credit pages, transcribed verbatim from `START.EXE`'s credits
/// routine (`0x460b`, pages at data offsets `0x6e5`..`0xb05`). The original's
/// blank padding rows are dropped; the meaningful lines are centred.
const CREDIT_PAGES: &[&[&str]] = &[
    &[
        "PROGRAM:",
        "",
        "ERIK POJAR",
        "",
        "GRAPHICS:",
        "",
        "MICHAEL SORMANN",
        "PETER BAUSTAEDTER",
    ],
    &[
        "MUSIC:",
        "",
        "NEO PROJECT",
        "HANNES SEIFERT",
        "PETER MELCHART",
        "",
        "DESIGN:",
        "",
        "MICHAEL SORMANN",
        "NIKI LABER",
    ],
    &[
        "ADDITIONAL CODING:",
        "",
        "CHRISTOPH SOUKUP",
        "PETER MELCHART",
        "",
        "GAME TESTING:",
        "",
        "MICHAELA STEURER",
        "VICTOR METYKO",
        "NIKI GHALUSTIAN",
        "KAWEH KAZEMI",
    ],
    &[
        "SPECIAL THANKS TO:",
        "",
        "OLIVER LEEDS",
        "CHRISTOPH BRUNMAYR",
        "KLAUS KRALL",
        "ALEXANDER CECH",
        "WOLFGANG TENGLER",
        "TOBIAS SICHERITZ",
        "MARCUS ERBER",
        "FA. NIEDERMEYER",
    ],
];

/// One glyph cell of the menu font.
const GLYPH: i32 = 16;

/// Draw a block of lines centred on the screen.
fn draw_centered(framebuffer: &mut Framebuffer, font: &Font, lines: &[&str]) {
    let height = lines.len() as i32 * GLYPH;
    let mut y = (SCREEN_HEIGHT as i32 - height) / 2;

    for line in lines {
        let width = line.len() as i32 * GLYPH;
        let x = (SCREEN_WIDTH as i32 - width) / 2;
        font.draw_into(&mut framebuffer.image, x, y, line);
        y += GLYPH;
    }
}

fn black() -> Palette {
    Palette {
        colors: [Rgb::default(); 256],
    }
}

fn blank_image() -> IndexedImage {
    IndexedImage::new(
        Dimensions::new(SCREEN_WIDTH, SCREEN_HEIGHT),
        vec![0u8; (SCREEN_WIDTH * SCREEN_HEIGHT) as usize],
    )
    .expect("blank 320x200 image matches its dimensions")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::{test_intro_assets, test_menu_assets};

    fn test_intro() -> Intro {
        Intro::new(Rc::new(test_intro_assets()), Rc::new(test_menu_assets()))
    }

    #[test]
    fn first_update_starts_the_title_theme_once() {
        let mut intro = test_intro();

        assert_eq!(
            intro.update(Duration::ZERO, &[]).audio,
            vec![AudioCommand::PlayTrack(TITLE_TRACK)]
        );
        assert!(intro.update(Duration::ZERO, &[]).audio.is_empty());
    }

    #[test]
    fn any_key_skips_to_the_menu() {
        let mut intro = test_intro();
        intro.update(Duration::ZERO, &[]); // consume the boot frame

        assert_eq!(
            intro.update(Duration::ZERO, &[KeyEvent::Enter]).transition,
            Some(Transition::To(SceneId::MainMenu))
        );
    }

    #[test]
    fn reports_animating() {
        assert!(test_intro().is_animating());
    }

    #[test]
    fn opens_on_a_black_hold_then_fades_the_neo_still_in() {
        let mut intro = test_intro();
        // The intro opens on the black hold where the music starts, then fades
        // neo in, then holds. Check the machinery, not colour (the synthetic
        // still has an all-zero palette).
        assert!(matches!(intro.active, Active::Hold(_)), "opens on black hold");

        intro.update(ticks(230), &[]); // past the 220-tick black hold
        assert!(matches!(intro.active, Active::Fade(_)), "then fades in");

        intro.update(ticks(230), &[]); // past the 220-tick fade
        assert!(matches!(intro.active, Active::Hold(_)), "then holds");
    }

    #[test]
    fn plays_through_to_the_menu_when_left_alone() {
        let mut intro = test_intro();

        // Synthetic FLIs are empty, so the FLI and credits beats skip; the
        // stills, fades and holds still run. Pump enough frames to exhaust the
        // whole script and assert it asks for the menu.
        let mut transition = None;

        for _ in 0..2000 {
            let output = intro.update(ticks(20), &[]);

            if output.transition.is_some() {
                transition = output.transition;
                break;
            }
        }

        assert_eq!(transition, Some(Transition::To(SceneId::MainMenu)));
    }

    #[test]
    #[cfg_attr(not(feature = "disc-tests"), ignore = "requires the disc image")]
    fn the_real_intro_decodes_every_fli_and_reaches_the_menu() {
        use prototype_disc::DiscImage;

        use crate::assets::{load_intro_assets, load_menu_assets};

        let disc = DiscImage::open_default().expect("disc image");
        let assets = load_intro_assets(&disc).expect("loading intro assets");
        let menu_assets = load_menu_assets(&disc).expect("loading menu assets");
        let mut intro = Intro::new(Rc::new(assets), Rc::new(menu_assets));

        // Step in 1/70 s frames over a generous wall-clock budget. The real
        // FLIs decode as their beats start; the run must end at the menu rather
        // than stall, which would mean a beat never finished.
        let mut transition = None;

        for _ in 0..100_000 {
            let output = intro.update(ticks(1), &[]);

            if output.transition.is_some() {
                transition = output.transition;
                break;
            }
        }

        assert_eq!(transition, Some(Transition::To(SceneId::MainMenu)));
    }
}
