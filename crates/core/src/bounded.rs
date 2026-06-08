//! A small bounded integer for gameplay counters.

/// A `u8` clamped to `0..=BOUND`, for capped gameplay counters.
///
/// The ceiling is enforced at construction and on [`saturating_add`], so a value
/// can never exceed `BOUND`; the floor is `0`. Domain counters are type aliases
/// over this (a weapon level, lives, smart bombs), distinguished by their bound.
/// When one needs behavior of its own, promote its alias to a real newtype
/// wrapping this type, keeping the same `new` / `get` surface.
///
/// [`saturating_add`]: BoundedU8::saturating_add
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BoundedU8<const BOUND: u8>(u8);

impl<const BOUND: u8> BoundedU8<BOUND> {
    /// The highest value this counter holds.
    pub const MAX: u8 = BOUND;

    /// Builds a value, clamping to `0..=BOUND`.
    pub fn new(value: u8) -> Self {
        Self(value.min(BOUND))
    }

    /// Returns the value as a plain `u8`.
    pub fn get(self) -> u8 {
        self.0
    }

    /// Adds `rhs`, clamped to the bound.
    pub fn saturating_add(self, rhs: u8) -> Self {
        Self::new(self.0.saturating_add(rhs))
    }

    /// Subtracts `rhs`, saturating at `0`.
    pub fn saturating_sub(self, rhs: u8) -> Self {
        Self(self.0.saturating_sub(rhs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type Four = BoundedU8<4>;

    #[test]
    fn new_clamps_above_the_bound() {
        assert_eq!(Four::new(7).get(), 4);
        assert_eq!(Four::new(4).get(), 4);
        assert_eq!(Four::new(2).get(), 2);
    }

    #[test]
    fn saturating_add_clamps_to_the_bound() {
        assert_eq!(Four::new(2).saturating_add(1).get(), 3);
        assert_eq!(Four::new(3).saturating_add(5).get(), 4);
        assert_eq!(Four::new(4).saturating_add(1).get(), 4);
    }

    #[test]
    fn saturating_sub_floors_at_zero() {
        assert_eq!(Four::new(3).saturating_sub(1).get(), 2);
        assert_eq!(Four::new(1).saturating_sub(4).get(), 0);
        assert_eq!(Four::new(0).saturating_sub(1).get(), 0);
    }

    #[test]
    fn max_reports_the_bound() {
        assert_eq!(Four::MAX, 4);
        assert_eq!(BoundedU8::<9>::MAX, 9);
    }
}
