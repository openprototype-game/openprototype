//! CD-DA track decoding against the real image.
//!
//! Verifies the audio-asset path the music player relies on: the disc exposes
//! the OST tracks and they decode to a non-empty, well-formed stereo `f32`
//! buffer. Gated on the image being present.

use prototype::assets::load_track_pcm_f32;
use prototype_integration_tests::open_test_image;

#[test]
#[cfg_attr(not(feature = "disc-tests"), ignore = "requires the disc image")]
fn title_theme_track_decodes_to_stereo_samples() {
    let image = open_test_image();

    // Track 2 is the title theme (track 1 is data; the music is tracks 2..=8).
    let samples = load_track_pcm_f32(&image, 2).expect("track 2 decodes");

    assert!(!samples.is_empty(), "track 2 produced no samples");
    assert!(
        samples.len().is_multiple_of(2),
        "interleaved stereo must have an even sample count, got {}",
        samples.len()
    );
    assert!(
        samples.iter().all(|sample| (-1.0..1.0).contains(sample)),
        "every sample must be normalized into [-1.0, 1.0)"
    );
}

#[test]
#[cfg_attr(not(feature = "disc-tests"), ignore = "requires the disc image")]
fn all_seven_ost_tracks_are_present() {
    let image = open_test_image();

    for track in 2..=8 {
        let samples = load_track_pcm_f32(&image, track)
            .unwrap_or_else(|error| panic!("track {track} should decode: {error:#}"));
        assert!(!samples.is_empty(), "track {track} produced no samples");
    }
}
