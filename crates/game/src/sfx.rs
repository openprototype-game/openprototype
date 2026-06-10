//! Sound-effect triggers: which sample plays on which mixer channel.
//!
//! Reverse-engineered from `LEVEL_1.WAD` (trigger battery file `0xac83..`,
//! mixer feed `0x7a80`) and byte-verified across all seven WADs
//! (`re/find_sfx_per_level.py`). The original mixes three channels into the
//! Sound Blaster's DMA buffer and assigns them by sound category: explosions
//! and impacts on 0, the player's weapons on 1, pickups/enemy/weapon-switch
//! events on 2. A trigger overwrites its channel immediately; there is no
//! fade and no priority (except the chainhit don't-interrupt guard, not yet
//! wired).
//!
//! The plasma weapon's audio is the burning2 hum: a looped sample started on
//! the first firing tick, guarded by the original's `cs:[0x8414]` flag so the
//! per-tick dispatch never restarts it, and released when the plasma ball
//! launches (the launch itself is silent; the hum's current pass plays out).
//! The orb bolts are silent.
//!
//! Samples are addressed by their slot in the WAD's filename table (see
//! [`SfxData`](crate::levels::SfxData)); slot meanings are positional and
//! identical in every level.

use std::sync::Arc;

use openprototype_core::audio::{AudioCommand, PlaySfx};
use openprototype_core::{ActiveWeapon, Weapon};

/// The player-weapon mixer channel.
const WEAPON_CHANNEL: usize = 1;
/// The pickups/enemy/weapon-switch mixer channel.
const EVENT_CHANNEL: usize = 2;

/// Sample slots in the WAD's filename table.
const SLOT_CHAINGUN: usize = 0;
const SLOT_CHANGEWE: usize = 2;
const SLOT_MISSILE: usize = 10;
const SLOT_PLASMAGU: usize = 11;
const SLOT_BURNING2: usize = 14;
const SLOT_MULTISHO: usize = 15;

/// The level's loaded samples, indexed by slot, each already cut to its
/// trigger's authored length.
pub struct SfxBank {
    pub samples: Vec<Arc<[i8]>>,
}

/// The trigger state: the plasma hum's loop flag.
#[derive(Default)]
pub struct Sfx {
    /// The original's `cs:[0x8414]`: set when the hum starts so the per-tick
    /// plasma dispatch does not restart it, cleared when the ball launches.
    hum_looping: bool,
}

impl Sfx {
    pub fn new() -> Self {
        Self { hum_looping: false }
    }

    /// The firing weapon spawned shots this tick. Every weapon restarts its
    /// sample on the weapon channel per shot; the plasma weapon instead keeps
    /// its hum looping.
    pub fn weapon_fired(
        &mut self,
        weapon: ActiveWeapon,
        bank: &SfxBank,
        audio: &mut Vec<AudioCommand>,
    ) {
        match weapon {
            ActiveWeapon::Chaingun => play(bank, SLOT_CHAINGUN, WEAPON_CHANNEL, false, audio),
            ActiveWeapon::Selected(Weapon::Multishot) => {
                play(bank, SLOT_MULTISHO, WEAPON_CHANNEL, false, audio);
            }
            ActiveWeapon::Selected(Weapon::Burning) => {
                play(bank, SLOT_PLASMAGU, WEAPON_CHANNEL, false, audio);
            }
            ActiveWeapon::Selected(Weapon::Plasma) => {
                if !self.hum_looping {
                    self.hum_looping = true;
                    play(bank, SLOT_BURNING2, WEAPON_CHANNEL, true, audio);
                }
            }
            ActiveWeapon::Selected(Weapon::Missile) => {
                play(bank, SLOT_MISSILE, WEAPON_CHANNEL, false, audio);
            }
        }
    }

    /// The plasma ball launched: the hum's loop flag clears and its current
    /// pass plays out. The launch spawns no sound of its own.
    pub fn plasma_launched(&mut self, audio: &mut Vec<AudioCommand>) {
        self.hum_looping = false;
        audio.push(AudioCommand::EndSfxLoop {
            channel: WEAPON_CHANNEL,
        });
    }

    /// The firing weapon resolved to a different one (the original's per-tick
    /// resolve, file `0xae59`, plays the switch sound on that change rather
    /// than on the switch key itself).
    pub fn weapon_switched(&self, bank: &SfxBank, audio: &mut Vec<AudioCommand>) {
        play(bank, SLOT_CHANGEWE, EVENT_CHANNEL, false, audio);
    }
}

fn play(bank: &SfxBank, slot: usize, channel: usize, looped: bool, audio: &mut Vec<AudioCommand>) {
    audio.push(AudioCommand::PlaySfx(PlaySfx {
        channel,
        sample: bank.samples[slot].clone(),
        looped,
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bank() -> SfxBank {
        SfxBank {
            // Each slot's sample is one byte holding its own slot number, so a
            // command's sample identifies the slot it came from.
            samples: (0..16).map(|slot| Arc::from(vec![slot as i8])).collect(),
        }
    }

    fn played_slot(command: &AudioCommand) -> (usize, usize, bool) {
        match command {
            AudioCommand::PlaySfx(play) => (play.sample[0] as usize, play.channel, play.looped),
            other => panic!("expected PlaySfx, got {other:?}"),
        }
    }

    #[test]
    fn each_weapon_restarts_its_sample_on_the_weapon_channel() {
        let bank = bank();
        let cases = [
            (ActiveWeapon::Chaingun, SLOT_CHAINGUN),
            (ActiveWeapon::Selected(Weapon::Multishot), SLOT_MULTISHO),
            (ActiveWeapon::Selected(Weapon::Burning), SLOT_PLASMAGU),
            (ActiveWeapon::Selected(Weapon::Missile), SLOT_MISSILE),
        ];

        for (weapon, slot) in cases {
            let mut sfx = Sfx::new();
            let mut audio = Vec::new();
            sfx.weapon_fired(weapon, &bank, &mut audio);
            sfx.weapon_fired(weapon, &bank, &mut audio);

            assert_eq!(audio.len(), 2, "{weapon:?} restarts per shot");
            assert_eq!(played_slot(&audio[0]), (slot, WEAPON_CHANNEL, false));
        }
    }

    #[test]
    fn the_plasma_hum_starts_once_and_loops() {
        let bank = bank();
        let mut sfx = Sfx::new();
        let mut audio = Vec::new();

        // The plasma dispatch fires every held tick; the hum's guard flag
        // keeps it to one looped trigger.
        for _ in 0..5 {
            sfx.weapon_fired(ActiveWeapon::Selected(Weapon::Plasma), &bank, &mut audio);
        }

        assert_eq!(audio.len(), 1);
        assert_eq!(
            played_slot(&audio[0]),
            (SLOT_BURNING2, WEAPON_CHANNEL, true)
        );
    }

    #[test]
    fn the_launch_ends_the_hum_and_rearms_it() {
        let bank = bank();
        let mut sfx = Sfx::new();
        let mut audio = Vec::new();

        sfx.weapon_fired(ActiveWeapon::Selected(Weapon::Plasma), &bank, &mut audio);
        sfx.plasma_launched(&mut audio);

        assert_eq!(
            audio[1],
            AudioCommand::EndSfxLoop {
                channel: WEAPON_CHANNEL
            }
        );

        // The next plasma burst hums again.
        sfx.weapon_fired(ActiveWeapon::Selected(Weapon::Plasma), &bank, &mut audio);
        assert_eq!(audio.len(), 3);
        assert_eq!(
            played_slot(&audio[2]),
            (SLOT_BURNING2, WEAPON_CHANNEL, true)
        );
    }

    #[test]
    fn switching_weapons_plays_on_the_event_channel() {
        let bank = bank();
        let sfx = Sfx::new();
        let mut audio = Vec::new();

        sfx.weapon_switched(&bank, &mut audio);

        assert_eq!(
            played_slot(&audio[0]),
            (SLOT_CHANGEWE, EVENT_CHANNEL, false)
        );
    }
}
