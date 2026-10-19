//! Which copy of a survivor belongs to which altar.
//!
//! A slot is a survivor and an altar, and holds at most one entity: a
//! cluster of N altars can therefore hold at most N copies of a survivor,
//! and a copy only ever stands inside a zone. The original, the entity the
//! game's own record points at, is never touched: survivors not yet rescued
//! live outside every zone, and vanilla keeps them.
//!
//! Pure logic in whole 0.5 m cells, free of the game, so it can be tested.

use std::collections::BTreeMap;

use zone_geometry::Box3;

/// A survivor's id, the hash of its template.
pub type Survivor = u32;
pub type Entity = u32;
/// An altar's identity, as the altar mod keys it.
pub type AltarKey = u64;

/// What to do with an entity the uniqueness pass sees.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Keep,
    /// A second copy in a slot that already has one, or a copy left outside
    /// every zone.
    Remove,
}

#[derive(Default)]
pub struct Copies {
    /// The altars' zones, as the altar mod grows them.
    altars: Vec<(AltarKey, Box3)>,
    slots: BTreeMap<(Survivor, AltarKey), Entity>,
    /// Each kept entity's survivor and home.
    homes: BTreeMap<Entity, (Survivor, [i64; 3])>,
}

impl Copies {
    pub fn set_altars(&mut self, altars: impl IntoIterator<Item = (AltarKey, Box3)>) {
        self.altars = altars.into_iter().collect();
        self.altars.sort_by_key(|(key, _)| *key);
        let known: Vec<AltarKey> = self.altars.iter().map(|(k, _)| *k).collect();
        self.slots
            .retain(|(_, altar), _| known.binary_search(altar).is_ok());
    }

    /// The altar whose zone holds `point`; with overlapping zones, the one
    /// with the smallest key, so the answer never depends on order.
    pub fn altar_at(&self, point: [i64; 3]) -> Option<AltarKey> {
        self.altars
            .iter()
            .find(|(_, zone)| (0..3).all(|a| zone.min[a] <= point[a] && point[a] < zone.max[a]))
            .map(|(key, _)| *key)
    }

    /// The entity a summoning staff aimed at `target` should move, or `None`
    /// to spawn a new copy there. `Some(original)` outside every zone, so the
    /// staff behaves as vanilla there.
    pub fn for_spawn(
        &self,
        survivor: Survivor,
        target: [i64; 3],
        original: Entity,
    ) -> Option<Entity> {
        match self.altar_at(target) {
            Some(altar) => self.slots.get(&(survivor, altar)).copied(),
            None => (original != 0).then_some(original),
        }
    }

    /// The copy a bed or an order at `point` belongs to: the copy whose home
    /// is nearest, so a bed and its sleeper agree even where zones overlap;
    /// the original outside every zone.
    pub fn at(&self, survivor: Survivor, point: [i64; 3], original: Entity) -> Entity {
        if self.altar_at(point).is_none() {
            return original;
        }
        let distance = |home: &[i64; 3]| -> i128 {
            (0..3).map(|a| ((home[a] - point[a]) as i128).pow(2)).sum()
        };
        self.homes
            .iter()
            .filter(|(_, (s, _))| *s == survivor)
            .min_by_key(|(e, (_, home))| (distance(home), **e))
            .map(|(e, _)| *e)
            .unwrap_or(original)
    }

    /// Decides about `entity`, a copy of `survivor` standing at `home`, and
    /// files it in its slot when it may stay. `original` is the entity the
    /// game's own record points at.
    pub fn observe(
        &mut self,
        survivor: Survivor,
        entity: Entity,
        home: [i64; 3],
        original: Entity,
    ) -> Verdict {
        let altar = self.altar_at(home);
        self.homes.insert(entity, (survivor, home));
        // A copy moved away keeps no slot behind it.
        self.slots
            .retain(|(s, a), e| !(*s == survivor && *e == entity && Some(*a) != altar));
        let Some(altar) = altar else {
            if entity == original || original == 0 {
                return Verdict::Keep;
            }
            self.homes.remove(&entity);
            return Verdict::Remove;
        };
        match self.slots.get(&(survivor, altar)) {
            None => {
                self.slots.insert((survivor, altar), entity);
                Verdict::Keep
            }
            Some(&e) if e == entity => Verdict::Keep,
            // The original wins its own slot back.
            Some(_) if entity == original => {
                self.slots.insert((survivor, altar), entity);
                Verdict::Keep
            }
            Some(_) => {
                self.homes.remove(&entity);
                Verdict::Remove
            }
        }
    }

    /// Forgets an entity the game destroyed.
    pub fn forget(&mut self, entity: Entity) {
        self.slots.retain(|_, e| *e != entity);
        self.homes.remove(&entity);
    }

    /// Every kept copy of `survivor` with its home.
    pub fn copies_of(&self, survivor: Survivor) -> Vec<(Entity, [i64; 3])> {
        self.homes
            .iter()
            .filter(|(_, (s, _))| *s == survivor)
            .map(|(&e, &(_, home))| (e, home))
            .collect()
    }

    /// Whether any altar zone is known yet.
    pub fn knows_altars(&self) -> bool {
        !self.altars.is_empty()
    }

    /// How many copies of `survivor` stand in the zones.
    pub fn count(&self, survivor: Survivor) -> usize {
        self.slots.keys().filter(|(s, _)| *s == survivor).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SMITH: Survivor = 0x47a2_094a;

    fn zone(x: i64) -> Box3 {
        Box3::new([x, 0, 0], [x + 80, 80, 80])
    }

    fn two_altars() -> Copies {
        let mut copies = Copies::default();
        copies.set_altars([(1, zone(0)), (2, zone(80))]);
        copies
    }

    #[test]
    fn each_altar_gets_one_copy() {
        let mut copies = two_altars();
        assert_eq!(copies.observe(SMITH, 10, [10, 10, 10], 10), Verdict::Keep);
        assert_eq!(copies.observe(SMITH, 11, [90, 10, 10], 10), Verdict::Keep);
        assert_eq!(copies.observe(SMITH, 12, [95, 10, 10], 10), Verdict::Remove);
        assert_eq!(copies.count(SMITH), 2);
    }

    #[test]
    fn the_staff_spawns_where_no_copy_is_and_moves_where_one_is() {
        let mut copies = two_altars();
        copies.observe(SMITH, 10, [10, 10, 10], 10);
        assert_eq!(copies.for_spawn(SMITH, [50, 10, 10], 10), Some(10));
        assert_eq!(copies.for_spawn(SMITH, [90, 10, 10], 10), None);
        // Outside every zone the staff moves the original, as vanilla does.
        assert_eq!(copies.for_spawn(SMITH, [500, 10, 10], 10), Some(10));
    }

    #[test]
    fn the_original_is_never_removed() {
        let mut copies = two_altars();
        assert_eq!(copies.observe(SMITH, 11, [10, 10, 10], 10), Verdict::Keep);
        // The original shows up in the same altar: it takes the slot back.
        assert_eq!(copies.observe(SMITH, 10, [20, 10, 10], 10), Verdict::Keep);
        assert_eq!(copies.observe(SMITH, 11, [10, 10, 10], 10), Verdict::Remove);
        // Out in the world, not yet rescued, it stays too.
        assert_eq!(copies.observe(SMITH, 10, [900, 10, 10], 10), Verdict::Keep);
    }

    #[test]
    fn a_copy_outside_every_zone_goes() {
        let mut copies = two_altars();
        copies.observe(SMITH, 11, [90, 10, 10], 10);
        assert_eq!(
            copies.observe(SMITH, 11, [900, 10, 10], 10),
            Verdict::Remove
        );
        assert_eq!(copies.count(SMITH), 0);
    }

    #[test]
    fn overlapping_zones_pick_the_smallest_key() {
        let mut copies = Copies::default();
        copies.set_altars([(7, zone(0)), (3, zone(40))]);
        assert_eq!(copies.altar_at([50, 10, 10]), Some(3));
        assert_eq!(copies.altar_at([10, 10, 10]), Some(7));
    }

    #[test]
    fn beds_and_orders_find_the_copy_of_their_altar() {
        let mut copies = two_altars();
        copies.observe(SMITH, 10, [10, 10, 10], 10);
        copies.observe(SMITH, 11, [90, 10, 10], 10);
        assert_eq!(copies.at(SMITH, [100, 10, 10], 10), 11);
        assert_eq!(copies.at(SMITH, [20, 10, 10], 10), 10);
        assert_eq!(copies.at(SMITH, [900, 10, 10], 10), 10);
    }

    #[test]
    fn where_zones_overlap_a_bed_belongs_to_the_nearest_copy() {
        let mut copies = Copies::default();
        let (a, b) = (
            Box3::new([0, 0, 0], [80, 80, 80]),
            Box3::new([40, 0, 0], [120, 80, 80]),
        );
        copies.set_altars([(1, a), (2, b)]);
        copies.observe(SMITH, 10, [45, 10, 10], 10);
        copies.observe(SMITH, 11, [110, 10, 10], 10);
        // In both zones, nearer the original's home: the original's bed.
        assert_eq!(copies.at(SMITH, [50, 10, 10], 10), 10);
        assert_eq!(copies.at(SMITH, [100, 10, 10], 10), 11);
    }

    #[test]
    fn a_removed_altar_frees_its_slots() {
        let mut copies = two_altars();
        copies.observe(SMITH, 11, [90, 10, 10], 10);
        copies.set_altars([(1, zone(0))]);
        assert_eq!(copies.count(SMITH), 0);
        assert_eq!(copies.observe(SMITH, 11, [90, 10, 10], 10), Verdict::Remove);
    }

    #[test]
    fn a_destroyed_entity_frees_its_slot() {
        let mut copies = two_altars();
        copies.observe(SMITH, 11, [90, 10, 10], 10);
        copies.forget(11);
        assert_eq!(copies.for_spawn(SMITH, [90, 10, 10], 10), None);
    }
}
