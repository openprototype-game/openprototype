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
use crate::spawns::{Entity, Spawns, descriptor_hitboxes};

/// What a combat pass tells the scene.
#[derive(Default)]
pub struct CombatEvents {
    /// Score earned from kills this pass.
    pub score: u32,
    /// A type at or past the level-end descriptor died (the original sets
    /// `cs:0xcc1/0xcc2`).
    pub level_end: bool,
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
    weapons.shots.retain_mut(|shot| {
        let (size_x, size_y) = shot.collision_size();
        let budget = shot.damage;

        match apply_shot(
            &mut spawns.entities,
            shot.x >> 4,
            shot.y >> 4,
            size_x,
            size_y,
            budget,
        ) {
            ShotOutcome::Missed => true,
            ShotOutcome::Absorbed => false,
            ShotOutcome::Pierced(remaining) => {
                shot.damage = remaining;
                true
            }
        }
    });

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

        let entity = &spawns.entities[index];
        let kind = entity.kind;

        // TODO: boss/orbiter gate decrement for kinds 0x392e..=0x399c.
        if kind >= 0x3ae8 {
            events.level_end = true;
        }

        // TODO: per-type death SFX (0xad23, plus 0xad03/0xad43 specials) and
        // the debris template (descriptor +0x14) into the effects buffer.
        events.score += score_value(wad, cs_base, kind);

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
