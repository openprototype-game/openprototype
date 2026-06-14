//! Linear palette fades over time.
//!
//! Drives a fade from one 256-color palette to another over a fixed duration,
//! interpolating each channel. The intro fades the title and logo screens in
//! and out (to and from a black palette) the way `START.EXE`'s fade primitive
//! (`0x2ec4`) does.

use std::time::Duration;

use prototype_formats::{Palette, Rgb};

/// A palette fade in progress.
pub struct PaletteFade {
    from: Palette,
    to: Palette,
    duration: Duration,
    elapsed: Duration,
}

impl PaletteFade {
    /// Builds a fade from `from` to `to` over `duration`.
    pub fn new(from: Palette, to: Palette, duration: Duration) -> Self {
        Self {
            from,
            to,
            duration,
            elapsed: Duration::ZERO,
        }
    }

    /// Advances by `dt`, clamping at the end.
    ///
    /// Returns the part of `dt` past the fade's end, so the caller can roll it
    /// into whatever follows and beat boundaries lose no time.
    pub fn advance(&mut self, dt: Duration) -> Duration {
        let remaining = self.duration.saturating_sub(self.elapsed);
        self.elapsed = (self.elapsed + dt).min(self.duration);
        dt.saturating_sub(remaining)
    }

    /// Whether the fade has reached its end.
    pub fn finished(&self) -> bool {
        self.elapsed >= self.duration
    }

    /// The interpolated palette at the current point in the fade.
    pub fn current(&self) -> Palette {
        let progress = if self.duration.is_zero() {
            1.0
        } else {
            self.elapsed.as_secs_f32() / self.duration.as_secs_f32()
        };

        let mut colors = [Rgb::default(); 256];

        for (index, color) in colors.iter_mut().enumerate() {
            *color = lerp(self.from.colors[index], self.to.colors[index], progress);
        }

        Palette { colors }
    }
}

fn lerp(from: Rgb, to: Rgb, progress: f32) -> Rgb {
    Rgb {
        r: lerp_channel(from.r, to.r, progress),
        g: lerp_channel(from.g, to.g, progress),
        b: lerp_channel(from.b, to.b, progress),
    }
}

fn lerp_channel(from: u8, to: u8, progress: f32) -> u8 {
    let from = f32::from(from);
    let to = f32::from(to);
    (from + (to - from) * progress).round().clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(value: u8) -> Palette {
        Palette {
            colors: [Rgb {
                r: value,
                g: value,
                b: value,
            }; 256],
        }
    }

    #[test]
    fn interpolates_from_start_to_end() {
        let mut fade = PaletteFade::new(solid(0), solid(255), Duration::from_secs(1));
        assert_eq!(fade.current().colors[0].r, 0);

        fade.advance(Duration::from_millis(500));
        assert!((120..=135).contains(&fade.current().colors[0].r));
        assert!(!fade.finished());

        fade.advance(Duration::from_millis(500));
        assert_eq!(fade.current().colors[0].r, 255);
        assert!(fade.finished());
    }

    #[test]
    fn clamps_past_the_end() {
        let mut fade = PaletteFade::new(solid(0), solid(255), Duration::from_secs(1));
        fade.advance(Duration::from_secs(10));

        assert!(fade.finished());
        assert_eq!(fade.current().colors[0].r, 255);
    }

    #[test]
    fn returns_the_time_past_its_end() {
        let mut fade = PaletteFade::new(solid(0), solid(255), Duration::from_secs(1));

        assert_eq!(fade.advance(Duration::from_millis(600)), Duration::ZERO);
        assert_eq!(
            fade.advance(Duration::from_millis(600)),
            Duration::from_millis(200)
        );
        // Once finished, further time passes straight through.
        assert_eq!(
            fade.advance(Duration::from_millis(300)),
            Duration::from_millis(300)
        );
    }
}
