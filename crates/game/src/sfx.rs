//! Sound-effect triggers: which sample plays on which mixer channel.
//!
//! Reverse-engineered from `LEVEL_1.WAD` (trigger battery file `0xac83..`,
//! mixer feed `0x7a80`) and byte-verified across all seven WADs
//! (`re/find_sfx_per_level.py`). The original mixes three channels into the
//! Sound Blaster's DMA buffer and assigns them by sound category: explosions
//! and impacts on 0, the player's weapons on 1, pickups/enemy/weapon-switch
//! events on 2. A trigger overwrites its channel immediately; there is no
//! fade and no priority, except the chainhit don't-interrupt guard
//! (`skip_if_busy`, honored by the backend mixer).
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

use crate::spawns::BossExplosionSound;
use openprototype_core::audio::{AudioCommand, PlaySfx};
use openprototype_core::{ActiveWeapon, Weapon};

/// The explosions/impacts mixer channel.
const IMPACT_CHANNEL: usize = 0;
/// The player-weapon mixer channel.
const WEAPON_CHANNEL: usize = 1;
/// The pickups/enemy/weapon-switch mixer channel.
const EVENT_CHANNEL: usize = 2;

/// Sample slots in the WAD's filename table.
const SLOT_CHAINGUN: usize = 0;
const SLOT_CHAINHIT: usize = 1;
const SLOT_CHANGEWE: usize = 2;
const SLOT_POD_DEATH: usize = 3;
const SLOT_EXPLOSION: usize = 4;
const SLOT_ASTEROID_DEATH: usize = 5;
const SLOT_PEBBLE: usize = 6;
const SLOT_PICKUP: usize = 7;
const SLOT_MISSILE_IMPACT: usize = 9;
const SLOT_MISSILE: usize = 10;
const SLOT_PLASMAGU: usize = 11;
const SLOT_ENEMY_SHOT: usize = 12;
const SLOT_DRAIN: usize = 13;
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

    /// An enemy died: the big explosion, except the asteroid and carrier-pod
    /// types whose dedicated samples land on the same channel (the original
    /// plays both back to back; the second write wins the channel).
    pub fn enemy_died(&self, kind: u16, bank: &SfxBank, audio: &mut Vec<AudioCommand>) {
        let slot = match kind {
            0x3308 => SLOT_ASTEROID_DEATH,
            0x38b0 => SLOT_POD_DEATH,
            _ => SLOT_EXPLOSION,
        };

        play(bank, slot, IMPACT_CHANNEL, false, audio);
    }

    /// The ship died (the body-contact/shot consequence's `0xad23`).
    pub fn ship_died(&self, bank: &SfxBank, audio: &mut Vec<AudioCommand>) {
        play(bank, SLOT_EXPLOSION, IMPACT_CHANNEL, false, audio);
    }

    /// A pickup was collected (`0xacc3`).
    pub fn pickup_collected(&self, bank: &SfxBank, audio: &mut Vec<AudioCommand>) {
        play(bank, SLOT_PICKUP, EVENT_CHANNEL, false, audio);
    }

    /// A dying enemy converted into the weapon-upgrade pickup (`0xaca3`).
    pub fn weapon_upgrade_dropped(&self, bank: &SfxBank, audio: &mut Vec<AudioCommand>) {
        play(bank, SLOT_PEBBLE, EVENT_CHANNEL, false, audio);
    }

    /// The 10,000-point extra life: the score updater plays the pickup
    /// sample as the jingle (`0xacc3` at L1 file `0xb18e`).
    pub fn extra_life(&self, bank: &SfxBank, audio: &mut Vec<AudioCommand>) {
        play(bank, SLOT_PICKUP, EVENT_CHANNEL, false, audio);
    }

    /// A hit drained the firing weapon one level (`0xadde`).
    pub fn weapon_drained(&self, bank: &SfxBank, audio: &mut Vec<AudioCommand>) {
        play(bank, SLOT_DRAIN, EVENT_CHANNEL, false, audio);
    }

    /// The firing weapon's bar hit zero, or a ram zeroed it (`0xac83`).
    pub fn weapon_lost(&self, bank: &SfxBank, audio: &mut Vec<AudioCommand>) {
        play(bank, SLOT_CHANGEWE, EVENT_CHANNEL, false, audio);
    }

    /// An AI-triggered sample by slot (the per-level enemy voices, volleys
    /// and phase changes; the AI modules name their level's slots).
    pub fn ai_sound(&self, slot: usize, bank: &SfxBank, audio: &mut Vec<AudioCommand>) {
        play(bank, slot, EVENT_CHANNEL, false, audio);
    }

    /// An enemy fired a shot (`0xae33`).
    pub fn enemy_fired(&self, bank: &SfxBank, audio: &mut Vec<AudioCommand>) {
        play(bank, SLOT_ENEMY_SHOT, EVENT_CHANNEL, false, audio);
    }

    /// A boss explosion burst, with its level's samples (L1 `0xad43` plus
    /// the big `0xad23` in form 2; L3's bursts are the big explosion alone).
    pub fn boss_explosion(
        &self,
        sound: BossExplosionSound,
        bank: &SfxBank,
        audio: &mut Vec<AudioCommand>,
    ) {
        match sound {
            BossExplosionSound::AsteroidPair { form2 } => {
                play(bank, SLOT_ASTEROID_DEATH, IMPACT_CHANNEL, false, audio);

                if form2 {
                    play(bank, SLOT_EXPLOSION, IMPACT_CHANNEL, false, audio);
                }
            }
            BossExplosionSound::Explosion => {
                play(bank, SLOT_EXPLOSION, IMPACT_CHANNEL, false, audio);
            }
        }
    }

    /// A chaingun round hit an enemy (`0xad83`): the don't-interrupt guard
    /// drops it while the impact channel is still playing, so explosions are
    /// never cut short by the round stream.
    pub fn chaingun_impact(&self, bank: &SfxBank, audio: &mut Vec<AudioCommand>) {
        audio.push(AudioCommand::PlaySfx(PlaySfx {
            channel: IMPACT_CHANNEL,
            sample: bank.samples[SLOT_CHAINHIT].clone(),
            looped: false,
            skip_if_busy: true,
        }));
    }

    /// A missile hit an enemy (`0xad63`).
    pub fn missile_impact(&self, bank: &SfxBank, audio: &mut Vec<AudioCommand>) {
        play(bank, SLOT_MISSILE_IMPACT, IMPACT_CHANNEL, false, audio);
    }
}

fn play(bank: &SfxBank, slot: usize, channel: usize, looped: bool, audio: &mut Vec<AudioCommand>) {
    audio.push(AudioCommand::PlaySfx(PlaySfx {
        channel,
        sample: bank.samples[slot].clone(),
        looped,
        skip_if_busy: false,
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
