//! Flame altar clusters.
//!
//! Altars whose vanilla zones touch or overlap form a cluster. A cluster's
//! area budget is the sum of its members' vanilla footprints, so overlap is
//! never lost. Every member keeps one world-aligned box: all members of a
//! cluster grow by the same number of cells `k` on every horizontal side,
//! which is the same as growing the union of their boxes by `k`. The height
//! of every member becomes the tallest member's. Clusters whose grown
//! footprints touch merge and are recomputed until nothing changes.
//!
//! Everything is integer, in voxel-grid cells. Boxes are half-open
//! (`min <= p < max`), like the engine's point test. Axis 1 is up; the
//! footprint is axes 0 and 2. The result depends only on the set of altars,
//! never on their order.

/// A world-aligned, half-open box in voxel-grid cells. Axis 1 is up.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Box3 {
    pub min: [i64; 3],
    pub max: [i64; 3],
}

/// An altar and its vanilla zone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Altar {
    /// Stable identity, unique within the input. The smallest key in a
    /// cluster names the cluster.
    pub key: u64,
    pub zone: Box3,
}

/// An altar's zone after clustering.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placement {
    pub key: u64,
    /// The smallest key in the altar's cluster.
    pub cluster: u64,
    /// Cells every member of the cluster grew by on each horizontal side.
    pub growth: i64,
    pub zone: Box3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClusterError {
    DuplicateKey(u64),
    /// `min > max` on some axis.
    InvertedBox(u64),
}

impl Box3 {
    pub fn new(min: [i64; 3], max: [i64; 3]) -> Self {
        Self { min, max }
    }

    /// Footprint area, axes 0 and 2.
    pub fn footprint_area(&self) -> i128 {
        let x = (self.max[0] - self.min[0]).max(0) as i128;
        let z = (self.max[2] - self.min[2]).max(0) as i128;
        x * z
    }

    /// Whether the footprints touch or overlap. Sharing an edge or a corner
    /// counts as touching.
    pub fn footprint_touches(&self, other: &Box3) -> bool {
        [0, 2]
            .iter()
            .all(|&axis| self.min[axis] <= other.max[axis] && other.min[axis] <= self.max[axis])
    }

    /// The box grown by `k` cells on every horizontal side.
    pub fn grown(&self, k: i64) -> Box3 {
        let mut grown = *self;
        for axis in [0, 2] {
            grown.min[axis] -= k;
            grown.max[axis] += k;
        }
        grown
    }

    fn height(&self) -> i64 {
        self.max[1] - self.min[1]
    }

    /// The box made `height` tall around its own centre. An odd difference
    /// puts the extra cell on top.
    fn with_height(&self, height: i64) -> Box3 {
        let extra = height - self.height();
        let mut tall = *self;
        tall.min[1] -= extra / 2;
        tall.max[1] += extra - extra / 2;
        tall
    }
}

/// Area of the union of the boxes' footprints.
pub fn union_footprint_area(boxes: &[Box3]) -> i128 {
    let boxes: Vec<&Box3> = boxes.iter().filter(|b| b.footprint_area() > 0).collect();
    let mut xs: Vec<i64> = boxes.iter().flat_map(|b| [b.min[0], b.max[0]]).collect();
    xs.sort_unstable();
    xs.dedup();

    let mut area = 0;
    for strip in xs.windows(2) {
        let (x0, x1) = (strip[0], strip[1]);
        let mut spans: Vec<(i64, i64)> = boxes
            .iter()
            .filter(|b| b.min[0] <= x0 && x1 <= b.max[0])
            .map(|b| (b.min[2], b.max[2]))
            .collect();
        spans.sort_unstable();

        let mut covered: i128 = 0;
        let mut reached = i64::MIN;
        for (z0, z1) in spans {
            let from = z0.max(reached);
            if z1 > from {
                covered += (z1 - from) as i128;
            }
            reached = reached.max(z1);
        }
        area += covered * (x1 - x0) as i128;
    }
    area
}

/// Computes every altar's zone. The result is sorted by key.
pub fn clusters(altars: &[Altar]) -> Result<Vec<Placement>, ClusterError> {
    let mut altars = altars.to_vec();
    altars.sort_by_key(|a| a.key);
    if let Some(pair) = altars.windows(2).find(|pair| pair[0].key == pair[1].key) {
        return Err(ClusterError::DuplicateKey(pair[0].key));
    }
    if let Some(altar) = altars
        .iter()
        .find(|a| (0..3).any(|axis| a.zone.min[axis] > a.zone.max[axis]))
    {
        return Err(ClusterError::InvertedBox(altar.key));
    }

    // Altars with an empty footprint (level 0) take part in nothing.
    let active: Vec<usize> = (0..altars.len())
        .filter(|&i| altars[i].zone.footprint_area() > 0)
        .collect();
    let mut groups: Vec<Vec<usize>> = components(active.len(), |a, b| {
        altars[active[a]]
            .zone
            .footprint_touches(&altars[active[b]].zone)
    })
    .into_iter()
    .map(|group| group.into_iter().map(|i| active[i]).collect())
    .collect();

    // Each round either merges groups or stops, and the number of groups
    // only decreases, so this terminates.
    let grown = loop {
        let grown: Vec<(i64, Vec<Box3>)> =
            groups.iter().map(|group| grow(&altars, group)).collect();
        let merged = components(groups.len(), |a, b| {
            grown[a]
                .1
                .iter()
                .any(|x| grown[b].1.iter().any(|y| x.footprint_touches(y)))
        });
        if merged.len() == groups.len() {
            break grown;
        }
        groups = merged
            .into_iter()
            .map(|parts| {
                let mut members: Vec<usize> = parts
                    .into_iter()
                    .flat_map(|g| groups[g].iter().copied())
                    .collect();
                members.sort_unstable();
                members
            })
            .collect();
    };

    let mut placements: Vec<Placement> = altars
        .iter()
        .map(|a| Placement {
            key: a.key,
            cluster: a.key,
            growth: 0,
            zone: a.zone,
        })
        .collect();
    for (group, (growth, boxes)) in groups.iter().zip(grown) {
        // Members are sorted, so the first one has the smallest key.
        let cluster = altars[group[0]].key;
        for (&member, zone) in group.iter().zip(boxes) {
            placements[member] = Placement {
                key: altars[member].key,
                cluster,
                growth,
                zone,
            };
        }
    }
    Ok(placements)
}

/// The growth and the zones of one cluster, in the order of `members`.
fn grow(altars: &[Altar], members: &[usize]) -> (i64, Vec<Box3>) {
    let vanilla: Vec<Box3> = members.iter().map(|&i| altars[i].zone).collect();
    let budget: i128 = vanilla.iter().map(Box3::footprint_area).sum();
    let area =
        |k: i64| union_footprint_area(&vanilla.iter().map(|b| b.grown(k)).collect::<Vec<_>>());

    // The largest k whose area still fits the budget. The union never
    // exceeds the sum, so k = 0 always fits.
    let mut fits = 0;
    let mut exceeds = 1;
    while area(exceeds) <= budget {
        fits = exceeds;
        exceeds *= 2;
    }
    while exceeds - fits > 1 {
        let middle = fits + (exceeds - fits) / 2;
        if area(middle) <= budget {
            fits = middle;
        } else {
            exceeds = middle;
        }
    }

    // Round to the nearest step; half a step or more rounds up.
    let (below, above) = (area(fits), area(fits + 1));
    let growth = if 2 * (budget - below) < above - below {
        fits
    } else {
        fits + 1
    };

    let height = vanilla.iter().map(Box3::height).max().unwrap_or(0);
    let zones = vanilla
        .iter()
        .map(|b| b.grown(growth).with_height(height))
        .collect();
    (growth, zones)
}

/// Connected components of `0..n` under `touches`. Each component is sorted,
/// and components are ordered by their smallest member.
fn components(n: usize, touches: impl Fn(usize, usize) -> bool) -> Vec<Vec<usize>> {
    let mut parent: Vec<usize> = (0..n).collect();
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    for a in 0..n {
        for b in a + 1..n {
            if touches(a, b) {
                let (ra, rb) = (root(&mut parent, a), root(&mut parent, b));
                parent[ra.max(rb)] = ra.min(rb);
            }
        }
    }
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut slot = vec![usize::MAX; n];
    for i in 0..n {
        let r = root(&mut parent, i);
        if slot[r] == usize::MAX {
            slot[r] = groups.len();
            groups.push(Vec::new());
        }
        groups[slot[r]].push(i);
    }
    groups
}
