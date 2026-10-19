use proptest::prelude::*;
use zone_geometry::{Altar, Box3, ClusterError, Placement, clusters, union_footprint_area};

/// A square altar zone of `edge` cells with its corner at (x, z), height 40.
fn square(key: u64, x: i64, z: i64, edge: i64) -> Altar {
    Altar {
        key,
        zone: Box3::new([x, 0, z], [x + edge, 40, z + edge]),
    }
}

fn by_key(placements: &[Placement], key: u64) -> Placement {
    *placements.iter().find(|p| p.key == key).unwrap()
}

#[test]
fn a_lone_altar_keeps_its_zone() {
    let altar = square(7, 10, 10, 40);
    let result = clusters(&[altar]).unwrap();
    assert_eq!(
        result,
        vec![Placement {
            key: 7,
            cluster: 7,
            growth: 0,
            zone: altar.zone
        }]
    );
}

#[test]
fn no_altars_no_zones() {
    assert_eq!(clusters(&[]).unwrap(), vec![]);
}

#[test]
fn edge_to_edge_altars_form_one_cluster_without_growth() {
    let result = clusters(&[square(1, 0, 0, 40), square(2, 40, 0, 40)]).unwrap();
    assert_eq!(by_key(&result, 1).cluster, 1);
    assert_eq!(by_key(&result, 2).cluster, 1);
    assert_eq!(by_key(&result, 1).growth, 0);
    assert_eq!(by_key(&result, 2).zone, square(2, 40, 0, 40).zone);
}

#[test]
fn overlap_is_given_back_by_growth_rounded_to_the_nearest_step() {
    // Union 60 x 40 = 2400, budget 3200. Areas by k: k=3 gives 66 x 46 = 3036,
    // k=4 gives 68 x 48 = 3264. The budget is 164 into a step of 228, which is
    // more than half, so k rounds up to 4.
    let result = clusters(&[square(1, 0, 0, 40), square(2, 20, 0, 40)]).unwrap();
    let a = by_key(&result, 1);
    let b = by_key(&result, 2);
    assert_eq!((a.cluster, b.cluster), (1, 1));
    assert_eq!((a.growth, b.growth), (4, 4));
    assert_eq!(a.zone, Box3::new([-4, 0, -4], [44, 40, 44]));
    assert_eq!(b.zone, Box3::new([16, 0, -4], [64, 40, 44]));
}

#[test]
fn less_than_half_a_step_rounds_down() {
    // Overlap of 2 cells: union 78 x 40 = 3120, budget 3200. k=0 gives 3120,
    // k=1 gives 80 x 42 = 3360. 80 into a step of 240 is less than half.
    let result = clusters(&[square(1, 0, 0, 40), square(2, 38, 0, 40)]).unwrap();
    assert_eq!(by_key(&result, 1).growth, 0);
}

#[test]
fn exactly_half_a_step_rounds_up() {
    // Five 1 x 1 altars on the same cell: union 1, budget 5. k=0 gives 1,
    // k=1 gives 3 x 3 = 9. The budget is 4 into a step of 8, exactly half.
    let altars: Vec<Altar> = (1..=5)
        .map(|key| Altar {
            key,
            zone: Box3::new([0, 0, 0], [1, 1, 1]),
        })
        .collect();
    let result = clusters(&altars).unwrap();
    assert!(result.iter().all(|p| p.growth == 1 && p.cluster == 1));
}

#[test]
fn far_apart_altars_stay_separate() {
    let result = clusters(&[square(1, 0, 0, 40), square(2, 100, 0, 40)]).unwrap();
    assert_eq!(by_key(&result, 1).cluster, 1);
    assert_eq!(by_key(&result, 2).cluster, 2);
}

#[test]
fn clusters_whose_grown_shapes_touch_merge_and_are_recomputed() {
    // Altars 1 and 2 overlap and grow by 4 to x 64. Altar 3 starts at x 64:
    // its vanilla zone does not touch theirs (60 < 64), the grown one does.
    // Merged: union 2400 + 1600 = 4000, budget 4800. k=2 joins the two parts
    // into 108 x 44 = 4752, k=3 gives 110 x 46 = 5060: 48 of 308, down to 2.
    let result = clusters(&[
        square(1, 0, 0, 40),
        square(2, 20, 0, 40),
        square(3, 64, 0, 40),
    ])
    .unwrap();
    assert!(result.iter().all(|p| p.cluster == 1 && p.growth == 2));
}

#[test]
fn every_member_gets_the_tallest_height_around_its_own_centre() {
    let low = Altar {
        key: 1,
        zone: Box3::new([0, 10, 0], [40, 50, 40]),
    };
    let tall = Altar {
        key: 2,
        zone: Box3::new([40, 0, 0], [80, 80, 40]),
    };
    let result = clusters(&[low, tall]).unwrap();
    assert_eq!(
        by_key(&result, 1).zone,
        Box3::new([0, -10, 0], [40, 70, 40])
    );
    assert_eq!(by_key(&result, 2).zone, tall.zone);
}

#[test]
fn an_altar_inside_a_zone_adds_its_budget() {
    let result = clusters(&[square(1, 0, 0, 40), square(2, 10, 10, 20)]).unwrap();
    // Union 1600, budget 2000. A(1) = 42^2 = 1764, A(2) = 44^2 = 1936,
    // A(3) = 46^2 = 2116: 64 of 180, down to 2.
    assert!(result.iter().all(|p| p.cluster == 1 && p.growth == 2));
}

#[test]
fn empty_zones_take_part_in_nothing() {
    // A level 0 altar has a zone of size 0.
    let empty = Altar {
        key: 2,
        zone: Box3::new([40, 0, 0], [40, 0, 0]),
    };
    let result = clusters(&[square(1, 0, 0, 40), empty]).unwrap();
    assert_eq!(
        by_key(&result, 2),
        Placement {
            key: 2,
            cluster: 2,
            growth: 0,
            zone: empty.zone
        }
    );
    assert_eq!(by_key(&result, 1).zone, square(1, 0, 0, 40).zone);
}

#[test]
fn bad_input_is_an_error_not_a_panic() {
    assert_eq!(
        clusters(&[square(1, 0, 0, 40), square(1, 100, 0, 40)]),
        Err(ClusterError::DuplicateKey(1))
    );
    let inverted = Altar {
        key: 3,
        zone: Box3::new([10, 0, 0], [0, 40, 40]),
    };
    assert_eq!(clusters(&[inverted]), Err(ClusterError::InvertedBox(3)));
}

#[test]
fn union_area_counts_overlap_once() {
    let a = Box3::new([0, 0, 0], [40, 1, 40]);
    let b = Box3::new([20, 0, 20], [60, 1, 60]);
    assert_eq!(union_footprint_area(&[a, b]), 1600 + 1600 - 400);
    assert_eq!(union_footprint_area(&[]), 0);
}

fn altar_set() -> impl Strategy<Value = Vec<Altar>> {
    prop::collection::vec((-60i64..60, -60i64..60, 0i64..4, 0i64..40), 0..9).prop_map(|raw| {
        raw.into_iter()
            .enumerate()
            .map(|(i, (x, z, level, y))| {
                let edge = level * 10;
                Altar {
                    key: i as u64 * 3 + 1,
                    zone: Box3::new([x, y, z], [x + edge, y + edge, z + edge]),
                }
            })
            .collect()
    })
}

fn members(result: &[Placement], cluster: u64) -> Vec<Placement> {
    result
        .iter()
        .copied()
        .filter(|p| p.cluster == cluster)
        .collect()
}

proptest! {
    #[test]
    fn order_never_matters(altars in altar_set(), seed in any::<u64>()) {
        let mut shuffled = altars.clone();
        // A deterministic shuffle driven by the seed.
        let mut state = seed | 1;
        for i in (1..shuffled.len()).rev() {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            shuffled.swap(i, (state % (i as u64 + 1)) as usize);
        }
        prop_assert_eq!(clusters(&altars).unwrap(), clusters(&shuffled).unwrap());
    }

    #[test]
    fn zones_only_grow_and_members_grow_alike(altars in altar_set()) {
        let result = clusters(&altars).unwrap();
        prop_assert_eq!(result.len(), altars.len());
        for p in &result {
            let vanilla = altars.iter().find(|a| a.key == p.key).unwrap().zone;
            prop_assert!(p.growth >= 0);
            for axis in 0..3 {
                prop_assert!(p.zone.min[axis] <= vanilla.min[axis]);
                prop_assert!(p.zone.max[axis] >= vanilla.max[axis]);
            }
            if vanilla.footprint_area() > 0 {
                prop_assert_eq!(p.zone.min[0], vanilla.min[0] - p.growth);
                prop_assert_eq!(p.zone.max[2], vanilla.max[2] + p.growth);
            }
            for q in members(&result, p.cluster) {
                prop_assert_eq!(q.growth, p.growth);
                prop_assert_eq!(q.zone.max[1] - q.zone.min[1], p.zone.max[1] - p.zone.min[1]);
            }
        }
    }

    #[test]
    fn growth_is_the_nearest_step_to_the_budget(altars in altar_set()) {
        let result = clusters(&altars).unwrap();
        for p in &result {
            let vanilla: Vec<Box3> = members(&result, p.cluster)
                .iter()
                .map(|q| altars.iter().find(|a| a.key == q.key).unwrap().zone)
                .collect();
            let budget: i128 = vanilla.iter().map(Box3::footprint_area).sum();
            if budget == 0 {
                continue;
            }
            let area = |k: i64| {
                let grown: Vec<Box3> = vanilla.iter().map(|b| b.grown(k)).collect();
                union_footprint_area(&grown)
            };
            let miss = (area(p.growth) - budget).abs();
            prop_assert!(miss <= (area(p.growth + 1) - budget).abs());
            if p.growth > 0 {
                prop_assert!(miss <= (area(p.growth - 1) - budget).abs());
            }
        }
    }

    #[test]
    fn separate_clusters_never_touch(altars in altar_set()) {
        let result = clusters(&altars).unwrap();
        for p in &result {
            for q in &result {
                if p.cluster != q.cluster && p.zone.footprint_area() > 0 && q.zone.footprint_area() > 0 {
                    prop_assert!(!p.zone.footprint_touches(&q.zone));
                }
            }
        }
    }
}
