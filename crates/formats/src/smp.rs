//! `.SMP`: sound samples.
//!
//! Raw 8-bit PCM for SoundBlaster (no header): the whole file is mono 8-bit
//! samples. The developer mail (Erik Pojar) puts playback at roughly mono,
//! 8-bit, ~22 kHz. The on-disk sign convention is not recorded, so [`decode`]
//! takes the [`Encoding`] and normalises to unsigned 8-bit, which is what an
//! 8-bit WAV (and the SoundBlaster DAC) expects. The CD-audio music is not in
//! these files; it lived as red-book tracks on the original CD.

/// How the on-disk 8-bit samples represent silence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    /// Silence at 128. Used directly by 8-bit WAV.
    Unsigned,
    /// Silence at 0 (two's complement). Shifted by 128 to become unsigned.
    Signed,
}

/// Normalise raw SMP bytes to unsigned 8-bit PCM, ready for an 8-bit WAV.
pub fn decode(bytes: &[u8], encoding: Encoding) -> Vec<u8> {
    match encoding {
        Encoding::Unsigned => bytes.to_vec(),
        Encoding::Signed => bytes
            .iter()
            .map(|&sample| sample.wrapping_add(128))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsigned_passes_through() {
        let bytes = [0x00, 0x80, 0xFF, 0x42];
        assert_eq!(decode(&bytes, Encoding::Unsigned), bytes);
    }

    #[test]
    fn signed_shifts_silence_to_128() {
        // signed 0 -> 128, signed 127 -> 255, signed -128 (0x80) -> 0, signed -1 -> 127
        let bytes = [0x00, 0x7F, 0x80, 0xFF];
        assert_eq!(decode(&bytes, Encoding::Signed), [0x80, 0xFF, 0x00, 0x7F]);
    }
}
