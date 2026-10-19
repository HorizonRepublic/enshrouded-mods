//! Building across the seams between altars.
//!
//! Vanilla accepts an object only when both corners of its box lie in one
//! zone. Inside a cluster that is too strict: an object may sit across two
//! members' zones. The replacement accepts a box that the union of the
//! zones covers whole. Zones that together cover a connected box overlap or
//! touch, and the engine groups touching zones into one base, so a covered
//! box always lies in a single base.
//!
//! Coordinates are the engine's 32.32 fixed point. A zone is half-open,
//! `min - margin <= p < max + margin`, like the engine's point test; the
//! object's box is closed, its corners included, like vanilla's two-corner
//! test.

#[cfg(windows)]
pub(crate) use hook::install;

#[cfg(windows)]
mod hook {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use crate::builds::engine;
    use crate::warn;

    type Seam = unsafe extern "system" fn(zones: *const u8, corners: *const i64, margin: f32) -> u8;

    static ORIGINAL: AtomicUsize = AtomicUsize::new(0);
    static OFF: AtomicBool = AtomicBool::new(false);
    const FIXED_PER_METRE: f32 = 4294967296.0;

    pub(crate) fn install(target: usize) -> Result<(), String> {
        let original = unsafe { crate::hook::install(target, detour as Seam as usize) }?;
        ORIGINAL.store(original, Ordering::Release);
        Ok(())
    }

    unsafe extern "system" fn detour(zones: *const u8, corners: *const i64, margin: f32) -> u8 {
        let original: Seam = unsafe { std::mem::transmute(ORIGINAL.load(Ordering::Acquire)) };
        let vanilla = unsafe { original(zones, corners, margin) };
        if vanilla != 0 || OFF.load(Ordering::Acquire) {
            return vanilla;
        }
        match std::panic::catch_unwind(|| unsafe { across(zones, corners, margin) }) {
            Ok(inside) => inside as u8,
            Err(_) => {
                if !OFF.swap(true, Ordering::AcqRel) {
                    warn!("seam check off, vanilla from now on: it panicked");
                }
                vanilla
            }
        }
    }

    unsafe fn across(list: *const u8, corners: *const i64, margin: f32) -> bool {
        unsafe {
            let entries = list
                .add(engine::ZONE_LIST_ENTRIES)
                .cast::<*const u8>()
                .read();
            let count = list.add(engine::ZONE_LIST_COUNT).cast::<usize>().read();
            if entries.is_null() || count > 4096 {
                return false;
            }
            let mut zones = Vec::with_capacity(count);
            for i in 0..count {
                let entry = entries.add(i * engine::ZONE_ENTRY_SIZE);
                if entry.add(engine::ZONE_ENTRY_IGNORED).read() != 0
                    || entry.add(engine::ZONE_ENTRY_SHAPE).read() != 0
                {
                    continue;
                }
                let bounds = entry.cast::<[i64; 6]>().read_unaligned();
                zones.push((
                    [bounds[0], bounds[1], bounds[2]],
                    [bounds[3], bounds[4], bounds[5]],
                ));
            }
            let low = corners.cast::<[i64; 3]>().read_unaligned();
            let high = corners
                .cast::<u8>()
                .add(engine::CORNER_SIZE)
                .cast::<[i64; 3]>()
                .read_unaligned();
            let low_corner = [0, 1, 2].map(|a| low[a].min(high[a]));
            let high_corner = [0, 1, 2].map(|a| low[a].max(high[a]));
            super::covered(
                low_corner,
                high_corner,
                &zones,
                (margin * FIXED_PER_METRE) as i64,
            )
        }
    }
}

/// Whether the closed box `[low, high]` lies within the union of the
/// half-open `zones`, each widened by `margin` on every side.
pub fn covered(low: [i64; 3], high: [i64; 3], zones: &[([i64; 3], [i64; 3])], margin: i64) -> bool {
    if (0..3).any(|a| low[a] > high[a]) {
        return false;
    }
    let zones: Vec<([i64; 3], [i64; 3])> = zones
        .iter()
        .map(|(min, max)| {
            (
                min.map(|v| v.saturating_sub(margin)),
                max.map(|v| v.saturating_add(margin)),
            )
        })
        .filter(|(min, max)| (0..3).all(|a| min[a] <= high[a] && low[a] < max[a]))
        .collect();
    if zones.is_empty() {
        return false;
    }

    // Split the box at every zone face inside it; each piece is either
    // inside a zone or not. A piece is tested at its lowest point, and the
    // box's own high faces close the last piece on each axis.
    let cuts = |axis: usize| -> Vec<i64> {
        let mut c: Vec<i64> = zones
            .iter()
            .flat_map(|(min, max)| [min[axis], max[axis]])
            .filter(|&v| low[axis] < v && v <= high[axis])
            .chain([low[axis]])
            .collect();
        c.sort_unstable();
        c.dedup();
        c
    };
    let (xs, ys, zs) = (cuts(0), cuts(1), cuts(2));
    for &x in &xs {
        for &y in &ys {
            for &z in &zs {
                let p = [x, y, z];
                if !zones
                    .iter()
                    .any(|(min, max)| (0..3).all(|a| min[a] <= p[a] && p[a] < max[a]))
                {
                    return false;
                }
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zone(min: [i64; 3], max: [i64; 3]) -> ([i64; 3], [i64; 3]) {
        (min, max)
    }

    #[test]
    fn a_box_inside_one_zone_is_covered() {
        let zones = [zone([0, 0, 0], [10, 10, 10])];
        assert!(covered([1, 1, 1], [9, 9, 9], &zones, 0));
    }

    #[test]
    fn a_box_across_two_touching_zones_is_covered() {
        let zones = [
            zone([0, 0, 0], [10, 10, 10]),
            zone([10, 0, 0], [20, 10, 10]),
        ];
        assert!(covered([5, 1, 1], [15, 9, 9], &zones, 0));
    }

    #[test]
    fn the_far_face_is_outside_a_half_open_zone() {
        let zones = [zone([0, 0, 0], [10, 10, 10])];
        assert!(!covered([1, 1, 1], [10, 9, 9], &zones, 0));
        assert!(covered([1, 1, 1], [10, 9, 9], &zones, 1));
    }

    #[test]
    fn corners_in_two_zones_do_not_cover_the_gap_of_an_l() {
        // An L: the corner (15, 15) is in neither zone.
        let zones = [zone([0, 0, 0], [20, 10, 10]), zone([0, 0, 0], [10, 20, 10])];
        assert!(covered([1, 1, 1], [9, 19, 9], &zones, 0));
        assert!(!covered([1, 1, 1], [15, 15, 9], &zones, 0));
    }

    #[test]
    fn a_gap_between_zones_is_not_covered() {
        let zones = [
            zone([0, 0, 0], [10, 10, 10]),
            zone([11, 0, 0], [20, 10, 10]),
        ];
        assert!(!covered([5, 1, 1], [15, 9, 9], &zones, 0));
    }

    #[test]
    fn no_zone_covers_nothing() {
        assert!(!covered([0, 0, 0], [1, 1, 1], &[], 0));
    }
}
