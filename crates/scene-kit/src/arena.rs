//! Generational arena: stable handles with ABA-safe slot reuse.

use std::vec::Vec;

/// Handle to a slot in an [`Arena`]. Cheap to copy; safe to persist.
///
/// Generation tagging means a stale handle from a removed and reused slot
/// resolves to nothing instead of to the new occupant (REQ-S1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Id {
    idx: u32,
    generation: u32,
}

impl Id {
    /// Pack into a u64 (`idx:32 | generation:32`).
    #[must_use]
    pub const fn to_bits(self) -> u64 {
        ((self.idx as u64) << 32) | self.generation as u64
    }

    /// Unpack from [`Id::to_bits`].
    #[must_use]
    pub const fn from_bits(bits: u64) -> Self {
        Id {
            idx: (bits >> 32) as u32,
            generation: bits as u32,
        }
    }

    /// Slot index (for debugging and traversal order only).
    #[must_use]
    pub const fn index(self) -> u32 {
        self.idx
    }
}

#[derive(Debug)]
struct Slot<T> {
    generation: u32,
    value: Option<T>,
}

/// Generational arena over `T`.
#[derive(Debug)]
pub struct Arena<T> {
    slots: Vec<Slot<T>>,
    free: Vec<u32>,
    live: usize,
}

impl<T> Arena<T> {
    /// New empty arena.
    #[must_use]
    pub const fn new() -> Self {
        Arena {
            slots: Vec::new(),
            free: Vec::new(),
            live: 0,
        }
    }

    /// Insert a value, returning its handle.
    pub fn insert(&mut self, value: T) -> Id {
        self.live += 1;
        if let Some(idx) = self.free.pop() {
            let generation = match self.slots.get(idx as usize) {
                Some(slot) => slot.generation,
                None => 0,
            };
            if let Some(slot) = self.slots.get_mut(idx as usize) {
                slot.value = Some(value);
            }
            return Id { idx, generation };
        }
        let idx = u32::try_from(self.slots.len()).unwrap_or(u32::MAX);
        self.slots.push(Slot {
            generation: 0,
            value: Some(value),
        });
        Id { idx, generation: 0 }
    }

    /// Resolve a handle to a shared reference, or `None` when stale.
    #[must_use]
    pub fn get(&self, id: Id) -> Option<&T> {
        let slot = self.slots.get(id.idx as usize)?;
        if slot.generation != id.generation {
            return None;
        }
        slot.value.as_ref()
    }

    /// Resolve a handle to a unique reference, or `None` when stale.
    #[must_use]
    pub fn get_mut(&mut self, id: Id) -> Option<&mut T> {
        let slot = self.slots.get_mut(id.idx as usize)?;
        if slot.generation != id.generation {
            return None;
        }
        slot.value.as_mut()
    }

    /// Remove a value. The slot's generation bumps, invalidating every
    /// outstanding handle. Generation wraps retire the slot permanently.
    pub fn remove(&mut self, id: Id) -> Option<T> {
        let slot = self.slots.get_mut(id.idx as usize)?;
        if slot.generation != id.generation {
            return None;
        }
        let value = slot.value.take();
        let (next_gen, retire) = slot.generation.overflowing_add(1);
        slot.generation = if retire { u32::MAX } else { next_gen };
        if !retire {
            self.free.push(id.idx);
        }
        self.live -= 1;
        value
    }

    /// Live element count.
    #[must_use]
    pub fn len(&self) -> usize {
        self.live
    }

    /// True when no live elements.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.live == 0
    }

    /// Iterate live values (slot order, not insertion order).
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.slots.iter().filter_map(|s| s.value.as_ref())
    }
}

impl<T> Default for Arena<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
pub(crate) fn poison_id() -> Id {
    Id {
        idx: u32::MAX,
        generation: u32::MAX,
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn arena_generations() {
        let mut arena: Arena<u32> = Arena::new();
        let a = arena.insert(10);
        let b = arena.insert(20);
        assert_eq!(arena.get(a), Some(&10));
        assert_eq!(arena.get(b), Some(&20));

        assert_eq!(arena.remove(a), Some(10));
        assert_eq!(arena.get(a), None, "removed handle misses");

        let c = arena.insert(30);
        assert_eq!(c.index(), a.index(), "slot reused");
        assert_ne!(c.generation, a.generation, "generation bumped");
        assert_eq!(arena.get(a), None, "stale handle still misses");
        assert_eq!(arena.get(c), Some(&30));
    }

    #[test]
    fn remove_stale_is_none() {
        let mut arena: Arena<u32> = Arena::new();
        let a = arena.insert(1);
        arena.remove(a);
        assert_eq!(arena.remove(a), None);
        assert!(arena.is_empty());
        assert_eq!(arena.len(), 0);
    }

    #[test]
    fn bits_roundtrip() {
        let id = Id {
            idx: 0xdead_beef,
            generation: 0x1234_5678,
        };
        assert_eq!(Id::from_bits(id.to_bits()), id);
    }

    #[test]
    fn iter_skips_dead_slots() {
        let mut arena: Arena<u32> = Arena::new();
        let a = arena.insert(1);
        let _ = arena.insert(2);
        let c = arena.insert(3);
        arena.remove(a);
        arena.remove(c);
        let got: Vec<u32> = arena.iter().copied().collect();
        assert_eq!(got, vec![2]);
    }
}
