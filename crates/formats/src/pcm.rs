//! Decodes CD-DA PCM bytes into normalized `f32` samples.
//!
//! Red-book audio is interleaved 16-bit little-endian PCM. The disc reader hands
//! back those raw bytes (`prototype_disc::DiscImage::read_track_pcm`); the
//! playback backend turns them into `f32` in `[-1.0, 1.0)`. A trailing odd byte
//! (never expected in red-book audio) is dropped. The sibling [`smp`](crate::smp)
//! module decodes the game's 8-bit SoundBlaster samples.

/// Converts raw little-endian 16-bit PCM into `f32` in `[-1.0, 1.0)`.
pub fn i16_le_to_f32(bytes: &[u8]) -> Vec<f32> {
    let mut out = Vec::with_capacity(bytes.len() / 2);
    append_i16_le_to_f32(bytes, &mut out);
    out
}

/// Appends little-endian 16-bit PCM from `bytes` to `out`, normalized.
///
/// Samples land in `[-1.0, 1.0)`. Lets a streaming source refill its buffer
/// without reallocating.
pub fn append_i16_le_to_f32(bytes: &[u8], out: &mut Vec<f32>) {
    out.extend(
        bytes
            .chunks_exact(2)
            .map(|pair| i16::from_le_bytes([pair[0], pair[1]]) as f32 / 32768.0),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_i16_le_to_normalized_f32() {
        // 0, i16::MIN, i16::MAX, little-endian.
        let bytes = [0x00, 0x00, 0x00, 0x80, 0xff, 0x7f];
        let samples = i16_le_to_f32(&bytes);

        assert_eq!(samples[0], 0.0);
        assert_eq!(samples[1], -1.0);
        assert!((samples[2] - 0.999_969).abs() < 1e-4);
    }

    #[test]
    fn drops_trailing_odd_byte() {
        assert_eq!(i16_le_to_f32(&[0x00, 0x00, 0x7f]).len(), 1);
    }
}
