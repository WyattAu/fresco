//! Memoized measurement cache with generation-based invalidation.

use alloc::collections::BTreeMap;

/// Memoized layout measurements.
///
/// Entries are keyed by `(node, input_hash)` and stamped with the
/// generation that must be current: a global generation plus a per-node
/// generation. `get_or_insert_with` recomputes only when the generation
/// advanced or the input hash changed (REQ-L1).
///
/// `K` is the caller's node handle; `V` the measurement value (typically
/// a size pair). Clones serve as cache reads, so `V` should be cheap.
#[derive(Debug)]
pub struct MeasureCache<K: Copy + Ord, V: Clone> {
    entries: BTreeMap<(K, u64), Entry<V>>,
    node_gens: BTreeMap<K, u32>,
    global_gen: u32,
    hits: u64,
    misses: u64,
}

#[derive(Debug, Clone)]
struct Entry<V> {
    generation: u32,
    input: u64,
    value: V,
}

impl<K: Copy + Ord, V: Clone> MeasureCache<K, V> {
    /// New empty cache.
    #[must_use]
    pub fn new() -> Self {
        MeasureCache {
            entries: BTreeMap::new(),
            node_gens: BTreeMap::new(),
            global_gen: 0,
            hits: 0,
            misses: 0,
        }
    }

    /// Required generation for a node: global generation plus node bumps.
    fn required_gen(&self, key: K) -> u32 {
        self.global_gen
            .wrapping_add(self.node_gens.get(&key).copied().unwrap_or(0))
    }

    /// Read or compute the measurement for `key` given `input_hash`.
    pub fn get_or_insert_with<F: FnOnce() -> V>(
        &mut self,
        key: K,
        input_hash: u64,
        compute: F,
    ) -> V {
        let want_gen = self.required_gen(key);
        if let Some(entry) = self.entries.get(&(key, input_hash))
            && entry.generation == want_gen
            && entry.input == input_hash
        {
            self.hits += 1;
            return entry.value.clone();
        }
        self.misses += 1;
        let value = compute();
        self.entries.insert(
            (key, input_hash),
            Entry {
                generation: want_gen,
                input: input_hash,
                value: value.clone(),
            },
        );
        value
    }

    /// Invalidate every entry for every node.
    pub fn invalidate_all(&mut self) {
        self.global_gen = self.global_gen.wrapping_add(1);
    }

    /// Invalidate one node.
    pub fn invalidate_node(&mut self, key: K) {
        let g = self.node_gens.entry(key).or_insert(0);
        *g = g.wrapping_add(1);
    }

    /// Drop stale entries so memory tracks the live working set.
    pub fn collect(&mut self) {
        let global = self.global_gen;
        let gens = &self.node_gens;
        self.entries.retain(|(key, _), entry| {
            global.wrapping_add(gens.get(key).copied().unwrap_or(0)) == entry.generation
        });
    }

    /// Entry count.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Cache hit count (measurement for tests and tuning).
    #[must_use]
    pub fn hits(&self) -> u64 {
        self.hits
    }

    /// Cache miss count.
    #[must_use]
    pub fn misses(&self) -> u64 {
        self.misses
    }
}

impl<K: Copy + Ord, V: Clone> Default for MeasureCache<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq)]
    struct Size {
        w: f64,
        h: f64,
    }

    #[test]
    fn hit_on_identical_query() {
        let mut cache = MeasureCache::<u32, Size>::new();
        let mut calls = 0;
        let v1 = cache.get_or_insert_with(1, 42, || {
            calls += 1;
            Size { w: 10.0, h: 5.0 }
        });
        let v2 = cache.get_or_insert_with(1, 42, || {
            calls += 1;
            Size { w: 10.0, h: 5.0 }
        });
        assert_eq!(calls, 1);
        assert_eq!(v1, v2);
        assert_eq!(cache.hits(), 1);
        assert_eq!(cache.misses(), 1);
    }

    #[test]
    fn input_hash_change_recomputes() {
        let mut cache = MeasureCache::<u32, Size>::new();
        let mut calls = 0;
        cache.get_or_insert_with(1, 42, || {
            calls += 1;
            Size { w: 10.0, h: 5.0 }
        });
        let v = cache.get_or_insert_with(1, 43, || {
            calls += 1;
            Size { w: 20.0, h: 5.0 }
        });
        assert_eq!(calls, 2);
        assert_eq!(v.w, 20.0);
    }

    #[test]
    fn node_invalidation_recomputes_only_that_node() {
        let mut cache = MeasureCache::<u32, Size>::new();
        let mut calls = 0;
        for node in 1..=2 {
            cache.get_or_insert_with(node, 42, || {
                calls += 1;
                Size { w: 1.0, h: 1.0 }
            });
        }
        cache.invalidate_node(1);
        cache.get_or_insert_with(1, 42, || {
            calls += 1;
            Size { w: 1.0, h: 1.0 }
        });
        cache.get_or_insert_with(2, 42, || {
            calls += 1;
            Size { w: 1.0, h: 1.0 }
        });
        assert_eq!(calls, 3, "only node 1 recomputed");
    }

    #[test]
    fn global_invalidation_recomputes_all() {
        let mut cache = MeasureCache::<u32, Size>::new();
        let mut calls = 0;
        for node in 1..=2 {
            cache.get_or_insert_with(node, 42, || {
                calls += 1;
                Size { w: 1.0, h: 1.0 }
            });
        }
        cache.invalidate_all();
        for node in 1..=2 {
            cache.get_or_insert_with(node, 42, || {
                calls += 1;
                Size { w: 1.0, h: 1.0 }
            });
        }
        assert_eq!(calls, 4);
    }

    #[test]
    fn collect_drops_stale_entries() {
        let mut cache = MeasureCache::<u32, Size>::new();
        cache.get_or_insert_with(1, 1, || Size { w: 1.0, h: 1.0 });
        cache.get_or_insert_with(2, 1, || Size { w: 2.0, h: 2.0 });
        assert_eq!(cache.len(), 2);
        cache.invalidate_node(1);
        cache.collect();
        assert_eq!(cache.len(), 1, "stale node entry collected");
    }
}
