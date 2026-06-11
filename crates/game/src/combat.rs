//! Player-fire combat: shots vs the enemy/pickup entities.
//!
//! Mirrors the original's buffer-A pass (file `0xc328`) and its hit test
//! (`0xbf47`) / damage application (`0xc0a4`): each shot is an AABB sized by
//! its spawner; per step it is tested against every live entity's three
//! collision boxes, and the first overlap spends the shot's damage budget.
//! Overkill pierces: the shot keeps the remainder and flies on. Deaths pay
//! the type's score (a dword in its descriptor block), and every Nth kill
//! converts the dying enemy into a weapon-orb pickup in place.
//!
//! Still TODO at their sites: hit sparks and death debris (the effects
//! buffer), death SFX triggers, and the boss/orbiter gate release.

use crate::shots::Weapons;
use crate::spawns::{Effect, Entity, Spawns, descriptor_debris, descriptor_hitboxes};
use openprototype_core::game_state::{GameState, HitOutcome, Severity};

/// The L1 WAD's ship hit-rect table (`cs:0x4771`), one pointer per roll band.
///
/// TODO: per-level offsets once the other levels' combat is wired (locate
/// like the spawn tables).
const SHIP_RECT_TABLE: usize = 0x4771;

/// What a combat pass tells the scene.
#[derive(Default)]
pub struct CombatEvents {
    /// Score earned from kills this pass.
    pub score: u32,
    /// A type at or past the level-end descriptor died (the original sets
    /// `cs:0xcc1/0xcc2`).
    pub level_end: bool,
    /// The worst thing that happened to the ship this pass.
    pub ship: Option<HitOutcome>,
    /// The ram consequence alone (for its distinct sound).
    pub ram: Option<HitOutcome>,
    /// The kinds that died this pass, in death order (the per-type death
    /// sounds share a channel, so the last one wins, like the original).
    pub kills: Vec<u16>,
    /// A kill converted into the weapon orb (`0xaca3`).
    pub orb_dropped: bool,
    /// A chaingun round / missile connected (`0xad83`/`0xad63`).
    pub chaingun_impact: bool,
    pub missile_impact: bool,
    /// A pickup was collected (`0xacc3`).
    pub pickup: bool,
    /// The invincibility pickup specifically (arms the shield visual).
    pub shield_pickup: bool,
}

/// Runs the player-shot pass: every live shot against every live entity.
///
/// The original moves shots and hit-tests per movement sub-step; the port
/// runs one pass per logic tick after both sides have moved.
pub fn player_shots(
    weapons: &mut Weapons,
    spawns: &mut Spawns,
    wad: &[u8],
    cs_base: usize,
    events: &mut CombatEvents,
) {
    let mut sparks = Vec::new();

    weapons.shots.retain_mut(|shot| {
        let (size_x, size_y) = shot.collision_size();
        let budget = shot.damage;

        let outcome = apply_shot(
            &mut spawns.entities,
            shot.x >> 4,
            shot.y >> 4,
            size_x,
            size_y,
            budget,
        );

        if outcome != ShotOutcome::Missed {
            events.chaingun_impact |= shot.is_chaingun();
            events.missile_impact |= shot.is_missile();

            // The hit spark by shot family (plasma sparks nothing).
            let x = shot.x >> 4;
            let y = shot.y >> 4;
            let spark = if shot.is_chaingun() {
                Some((0x356a, x - 0x20, y - 3, 6))
            } else if shot.is_missile() {
                Some((0x3522, x, y, 9))
            } else if shot.is_plasma() {
                None
            } else {
                Some((0x359a, x + 0x37, y - 3, 8))
            };

            if let Some((sprite, x, y, frames)) = spark {
                sparks.push(Effect {
                    sprite,
                    x,
                    y,
                    frames,
                    rate: 3,
                    step: 8,
                    phase: 0,
                    delay: 0,
                });
            }
        }

        match outcome {
            ShotOutcome::Missed => true,
            ShotOutcome::Absorbed => false,
            ShotOutcome::Pierced(remaining) => {
                shot.damage = remaining;
                true
            }
        }
    });

    for spark in sparks {
        spawns.push_effect(spark);
    }

    reap(spawns, wad, cs_base, events);
}

/// What one shot's hit test did this step.
#[derive(Debug, PartialEq, Eq)]
enum ShotOutcome {
    Missed,
    /// The hit enemy absorbed the full damage budget; the shot is culled.
    Absorbed,
    /// The hit enemy died with damage to spare; the shot flies on.
    Pierced(i32),
}

/// One shot AABB (pixel position and size) against the entity list, spending
/// `damage` on the first overlap (the original's `0xbf47`/`0xc0a4`).
fn apply_shot(
    entities: &mut [Entity],
    x: i32,
    y: i32,
    size_x: i32,
    size_y: i32,
    damage: i32,
) -> ShotOutcome {
    // Sizeless shots never collide, and the original rejects shots outside
    // the playfield before testing.
    if (size_x == 0 && size_y == 0) || !(0..=0x120).contains(&x) || !(0..=0xa0).contains(&y) {
        return ShotOutcome::Missed;
    }

    for entity in entities {
        if entity.health <= 0 {
            continue;
        }

        if !boxes_overlap(entity, x, y, size_x, size_y) {
            continue;
        }

        // TODO: hit spark into the effects buffer (skipped for plasma).
        if entity.health >= damage {
            entity.health -= damage;
            return ShotOutcome::Absorbed;
        }

        let remaining = damage - entity.health;
        entity.health = 0;
        return ShotOutcome::Pierced(remaining);
    }

    ShotOutcome::Missed
}

/// Tests a shot AABB against an entity's three collision boxes.
fn boxes_overlap(entity: &Entity, x: i32, y: i32, size_x: i32, size_y: i32) -> bool {
    let ex = entity.x >> 4;
    let ey = entity.y >> 4;

    entity.hitboxes.iter().any(|hitbox| {
        if hitbox[0] == 0xff {
            return false;
        }

        let x_min = ex + i32::from(hitbox[0]);
        let y_min = ey + i32::from(hitbox[1]);
        let x_max = ex + i32::from(hitbox[2]);
        let y_max = ey + i32::from(hitbox[3]);

        x + size_x > x_min && x <= x_max && y + size_y > y_min && y <= y_max
    })
}

/// Processes entities whose health reached zero: the death handler (file
/// `0xbda9`) plus the update loop's orb-drop conversion.
pub fn reap(spawns: &mut Spawns, wad: &[u8], cs_base: usize, events: &mut CombatEvents) {
    let mut index = 0;

    while index < spawns.entities.len() {
        if spawns.entities[index].health > 0 {
            index += 1;
            continue;
        }

        let kind = spawns.entities[index].kind;
        let sprite = spawns.entities[index].sprite;

        // The death handler keys these on the CURRENT sprite: a dying
        // orbiter frame releases one gate count, the boss/station range
        // flags the level end (which also bypasses the gate).
        if (0x392e..=0x399c).contains(&sprite) {
            spawns.gate = spawns.gate.saturating_sub(1);
        }

        if sprite >= 0x3ae8 {
            events.level_end = true;
            spawns.level_end = true;
        }

        events.kills.push(kind);
        events.score += score_value(wad, cs_base, kind);

        // The death debris: the entity's template rows burst at its pixel
        // position with their staggered delays.
        let px = spawns.entities[index].x >> 4;
        let py = spawns.entities[index].y >> 4;
        let debris = spawns.entities[index].debris;
        spawn_debris(spawns, wad, cs_base, debris, px, py);

        // A dying orb pickup is simply removed; everything else feeds the
        // every-Nth orb-drop countdown and may convert in place.
        if kind != 0x36ea && spawns.orb_drop_due() {
            let center = center_offset(wad, cs_base, kind);
            let entity = &mut spawns.entities[index];
            entity.sprite = 0x36ea;
            entity.kind = 0x36ea;
            entity.x += center.0;
            entity.y += center.1;
            entity.hitboxes = descriptor_hitboxes(wad, cs_base, 0x36ea);
            entity.debris = descriptor_debris(wad, cs_base, 0x36ea);
            entity.mode = 0;
            entity.arg = 5;
            entity.health = 0x15e;
            entity.seen = false;
            entity.anim = 0;
            entity.tick = 0;
            entity.phase_a = 0;
            entity.phase_b = 0;
            index += 1;
            continue;
        }

        spawns.entities.swap_remove(index);
    }
}

/// Bursts a death-debris template (a count byte, then 13-byte rows of
/// effect fields with position offsets) at the death's pixel position.
fn spawn_debris(spawns: &mut Spawns, wad: &[u8], cs_base: usize, template: u16, x: i32, y: i32) {
    let at = usize::from(template) + cs_base;

    if template == 0 || wad.len() < at + 1 {
        return;
    }

    let count = usize::from(wad[at]);

    for row in 0..count {
        let at = at + 1 + row * 13;

        if wad.len() < at + 13 {
            return;
        }

        let word = |k: usize| i32::from(i16::from_le_bytes([wad[at + k], wad[at + k + 1]]));
        spawns.push_effect(Effect {
            sprite: word(0) as u16,
            x: x + word(2),
            y: y + word(4),
            frames: wad[at + 6],
            rate: wad[at + 7],
            step: u16::from_le_bytes([wad[at + 8], wad[at + 9]]),
            phase: wad[at + 10],
            delay: u16::from_le_bytes([wad[at + 11], wad[at + 12]]),
        });
    }
}

/// The type's kill score: the dword at descriptor +0x16.
fn score_value(wad: &[u8], cs_base: usize, kind: u16) -> u32 {
    let at = usize::from(kind) + cs_base + 0x16;

    if wad.len() < at + 4 {
        return 0;
    }

    u32::from_le_bytes([wad[at], wad[at + 1], wad[at + 2], wad[at + 3]])
}

/// The type's center offset (descriptor +0x1a/+0x1c), where the dropped orb
/// appears.
fn center_offset(wad: &[u8], cs_base: usize, kind: u16) -> (i32, i32) {
    let at = usize::from(kind) + cs_base + 0x1a;

    if wad.len() < at + 4 {
        return (0, 0);
    }

    let word = |k: usize| i32::from(i16::from_le_bytes([wad[at + k], wad[at + k + 1]]));
    (word(0), word(2))
}

#[cfg(test)]
mod tests {
    use super::*;
    use openprototype_core::PerWeapon;
    use openprototype_core::game_state::{Lives, SmartBombs, Weapon, WeaponLevel};

    fn fresh_state() -> GameState {
        GameState {
            score: 0,
            lives: Lives::new(3),
            smart_bombs: SmartBombs::new(0),
            weapons: PerWeapon::splat(WeaponLevel::new(2)),
            selected: Weapon::Multishot,
            invincible_ticks: 0,
        }
    }

    /// Three rects all sitting on the ship at (100, 50).
    fn rects_at(x: i32, y: i32) -> ShipRects {
        [[x, y, x + 32, y + 24]; 3]
    }

    fn entity(x: i32, y: i32, health: i32) -> Entity {
        Entity {
            sprite: 0x3308,
            kind: 0x3308,
            x: x << 4,
            y: y << 4,
            mode: 0,
            arg: 0,
            health,
            seen: false,
            anim: 0,
            tick: 0,
            debris: 0,
            // One box covering 0..32 x 0..30 around the entity position.
            hitboxes: [[0, 0, 32, 30], [0xff, 0, 0, 0], [0xff, 0, 0, 0]],
            phase_a: 0,
            phase_b: 0,
            save_y: 0,
            save_x: 0,
        }
    }

    #[test]
    fn a_shot_damages_the_first_overlapping_entity() {
        let mut entities = vec![entity(100, 50, 100)];

        let outcome = apply_shot(&mut entities, 100, 50, 13, 4, 12);
        assert_eq!(outcome, ShotOutcome::Absorbed);
        assert_eq!(entities[0].health, 88);
    }

    #[test]
    fn overkill_pierces_with_the_remainder() {
        let mut entities = vec![entity(100, 50, 30)];

        let outcome = apply_shot(&mut entities, 100, 50, 13, 4, 80);
        assert_eq!(outcome, ShotOutcome::Pierced(50));
        assert_eq!(entities[0].health, 0);
    }

    #[test]
    fn disabled_boxes_and_misses_leave_the_shot_alone() {
        let mut entities = vec![entity(100, 50, 100)];
        entities[0].hitboxes[0][0] = 0xff;

        assert_eq!(
            apply_shot(&mut entities, 100, 50, 13, 4, 12),
            ShotOutcome::Missed
        );
        assert_eq!(entities[0].health, 100);

        entities[0].hitboxes[0][0] = 0;
        assert_eq!(
            apply_shot(&mut entities, 200, 50, 13, 4, 12),
            ShotOutcome::Missed
        );
    }

    #[test]
    fn touching_a_pickup_grants_and_removes_it() {
        let mut spawns = Spawns::new(Vec::new(), None);
        let mut orb = entity(100, 50, 350);
        orb.kind = 0x36ea;
        spawns.entities.push(orb);

        let mut state = fresh_state();
        let mut events = CombatEvents::default();
        body_contact(
            &mut spawns,
            &rects_at(100, 50),
            &mut state,
            &[],
            0,
            &mut events,
        );

        assert!(spawns.entities.is_empty());
        assert_eq!(state.level(Weapon::Multishot).get(), 3);
        assert_eq!(events.ship, None);
    }

    #[test]
    fn ramming_an_enemy_costs_the_bar_and_kills_it() {
        let mut spawns = Spawns::new(Vec::new(), None);
        spawns.entities.push(entity(100, 50, 100));

        let mut state = fresh_state();
        let mut events = CombatEvents::default();
        body_contact(
            &mut spawns,
            &rects_at(100, 50),
            &mut state,
            &[],
            0,
            &mut events,
        );

        // The selected bar zeroes, the rammed enemy dies and is reaped.
        assert_eq!(state.level(Weapon::Multishot).get(), 0);
        assert_eq!(events.ship, Some(HitOutcome::Absorbed));
        assert!(spawns.entities.is_empty() || spawns.entities[0].kind == 0x36ea);
    }

    #[test]
    fn an_exact_kill_is_absorbed_not_pierced() {
        // health == damage takes the >= branch: absorbed, health 0.
        let mut entities = vec![entity(100, 50, 12)];

        assert_eq!(
            apply_shot(&mut entities, 100, 50, 13, 4, 12),
            ShotOutcome::Absorbed
        );
        assert_eq!(entities[0].health, 0);
    }
}

/// One ship hit rect in screen pixels: `[x_min, y_min, x_max, y_max]`.
pub type ShipRects = [[i32; 4]; 3];

/// Builds the ship's three hit rects for the current roll frame (file
/// `0xda25`): the per-band block of byte offsets, anchored at the ship.
///
/// The original indexes the pointer table byte-granularly with `roll / 9`;
/// the roll counts in 0x12 steps, so that is `frame * 2` over a table that
/// duplicates each word. A `0xff` offset pushes the rect ~255 px away, which
/// disables it with no explicit check.
pub fn ship_rects(wad: &[u8], cs_base: usize, roll_frame: usize, x: i32, y: i32) -> ShipRects {
    let entry = SHIP_RECT_TABLE + cs_base + roll_frame * 2;

    if wad.len() < entry + 2 {
        return [[i32::MAX, i32::MAX, i32::MAX, i32::MAX]; 3];
    }

    let block = usize::from(u16::from_le_bytes([wad[entry], wad[entry + 1]])) + cs_base;

    std::array::from_fn(|rect| {
        let at = block + rect * 4;

        if wad.len() < at + 4 {
            return [i32::MAX, i32::MAX, i32::MAX, i32::MAX];
        }

        [
            x + i32::from(wad[at]),
            y + i32::from(wad[at + 1]),
            x + i32::from(wad[at + 2]),
            y + i32::from(wad[at + 3]),
        ]
    })
}

/// Runs the enemy-shot pass against the ship (file `0xc3f1`): each shot's
/// AABB against the three rects. Hits cull the shot regardless of
/// invincibility (the original sparks and culls before the consequence);
/// the returned count is how many hits the caller applies as
/// [`Severity::Bullet`].
pub fn enemy_shots_vs_ship(
    spawns: &mut Spawns,
    wad: &[u8],
    cs_base: usize,
    rects: &ShipRects,
) -> u32 {
    let mut hits = 0;
    let mut sparks = Vec::new();

    spawns.shots.retain(|shot| {
        let x = shot.x >> 4;
        let y = shot.y >> 4;

        if y < 0 {
            return true;
        }

        let (size_x, size_y) = shot_size(wad, cs_base, shot.sprite);
        let hit = rects.iter().any(|rect| {
            x + size_x > rect[0] && x <= rect[2] && y + size_y > rect[1] && y <= rect[3]
        });

        if hit {
            sparks.push(Effect {
                sprite: 0x34da,
                x: x - 5,
                y: y - 3,
                frames: 9,
                rate: 5,
                step: 8,
                phase: 0,
                delay: 0,
            });
            hits += 1;
        }

        !hit
    });

    for spark in sparks {
        spawns.push_effect(spark);
    }

    hits
}

/// An enemy shot's collision extent: the first two hitbox bytes of its
/// sprite descriptor (what the original's `spawn_shot` copies into the
/// record).
fn shot_size(wad: &[u8], cs_base: usize, sprite: u16) -> (i32, i32) {
    let at = usize::from(sprite) + cs_base + 8;

    if wad.len() < at + 2 {
        return (0, 0);
    }

    (i32::from(wad[at]), i32::from(wad[at + 1]))
}

/// Runs the body-contact pass (file `0xdae1`): every live entity's three
/// boxes against the three ship rects; the first overlap resolves.
///
/// Pickups grant immediately (the original routes them through a sentinel
/// health value and grants on the next update; the one-frame delay and its
/// smart-bomb corruption edge case are not reproduced). A rammed enemy costs
/// the ship a [`Severity::Collision`] hit and dies in place unless it is an
/// orbiter or the boss; the deaths run through [`reap`] with the rest.
pub fn body_contact(
    spawns: &mut Spawns,
    rects: &ShipRects,
    state: &mut GameState,
    wad: &[u8],
    cs_base: usize,
    events: &mut CombatEvents,
) {
    let mut rammed = false;
    let mut index = 0;

    while index < spawns.entities.len() {
        let entity = &spawns.entities[index];

        if entity.health <= 0 || !touches_ship(entity, rects) {
            index += 1;
            continue;
        }

        match entity.kind {
            // TODO: the HUD bar pickup effect.
            0x36ea => {
                state.level_up();
                events.pickup = true;
                spawns.entities.swap_remove(index);
            }
            0x3750 => {
                state.smart_bombs = state.smart_bombs.saturating_add(1);
                events.pickup = true;
                spawns.entities.swap_remove(index);
            }
            0x37b6 => {
                state.invincible_ticks = 600;
                events.pickup = true;
                events.shield_pickup = true;
                spawns.entities.swap_remove(index);
            }
            0x382c => {
                state.lives = state.lives.saturating_add(1);
                events.pickup = true;
                spawns.entities.swap_remove(index);
            }
            kind => {
                match state.take_hit(Severity::Collision) {
                    // Invincible: the ram has no effect on either side.
                    HitOutcome::Shielded => {}
                    outcome => {
                        events.ship = merge_ship_outcome(events.ship, outcome);
                        events.ram = merge_ship_outcome(events.ram, outcome);
                        rammed = true;

                        // The rammed enemy dies, except orbiters and the boss.
                        if kind != 0x392e && kind < 0x3ae8 {
                            spawns.entities[index].health = 0;
                        }
                    }
                }

                index += 1;
            }
        }
    }

    if rammed {
        reap(spawns, wad, cs_base, events);
    }
}

/// Tests an entity's three boxes against the three ship rects (9 overlap
/// tests; the original has no 0xff disable check here, the unsigned byte
/// offsets push disabled boxes out of reach instead).
fn touches_ship(entity: &Entity, rects: &ShipRects) -> bool {
    let ex = entity.x >> 4;
    let ey = entity.y >> 4;

    entity.hitboxes.iter().any(|hitbox| {
        let x_min = ex + i32::from(hitbox[0]);
        let y_min = ey + i32::from(hitbox[1]);
        let x_max = ex + i32::from(hitbox[2]);
        let y_max = ey + i32::from(hitbox[3]);

        rects.iter().any(|rect| {
            rect[2] >= x_min && rect[0] <= x_max && rect[3] >= y_min && rect[1] <= y_max
        })
    })
}

/// Keeps the worse of two ship outcomes across a pass.
fn merge_ship_outcome(current: Option<HitOutcome>, new: HitOutcome) -> Option<HitOutcome> {
    match (current, new) {
        (Some(HitOutcome::GameOver), _) | (_, HitOutcome::GameOver) => Some(HitOutcome::GameOver),
        (Some(HitOutcome::Died), _) | (_, HitOutcome::Died) => Some(HitOutcome::Died),
        (current, new) => current.or(Some(new)),
    }
}
