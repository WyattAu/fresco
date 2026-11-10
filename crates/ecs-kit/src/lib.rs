//! Entity-component-system for game-architecture composition.
//!
//! Entities are generational handles; components live in typed
//! storages keyed by `TypeId`; queries iterate in deterministic
//! entity order (BTreeMap-backed). Systems are plain functions the
//! caller drives — this crate provides world, storage, queries.
//!
//! Deterministic: same operation sequence produces identical query
//! output order, replay-safe by construction.

use std::any::{Any, TypeId};
use std::collections::BTreeMap;

/// A generational entity handle. Invalid after despawn; the
/// generation bump prevents stale-handle (ABA) aliasing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Entity {
    index: u32,
    generation: u32,
}

impl Entity {
    /// Pack into a u64 for storage keys.
    fn key(self) -> u64 {
        ((self.index as u64) << 32) | self.generation as u64
    }

    /// Unpack a storage key.
    fn from_key(key: u64) -> Self {
        Entity {
            index: (key >> 32) as u32,
            generation: key as u32,
        }
    }
}

/// Type-erased component storage interface.
trait StorageAny: Any {
    /// `Any` view for downcasting.
    fn as_any(&self) -> &dyn Any;
    /// Mutable `Any` view for downcasting.
    fn as_any_mut(&mut self) -> &mut dyn Any;
    /// Remove an entity's component; true when present.
    fn remove(&mut self, key: u64) -> bool;
    /// Whether the entity has this component.
    fn contains(&self, key: u64) -> bool;
    /// Number of live components.
    fn len(&self) -> usize;
}

impl<C: Component> StorageAny for BTreeMap<u64, C> {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn remove(&mut self, key: u64) -> bool {
        self.remove(&key).is_some()
    }

    fn contains(&self, key: u64) -> bool {
        self.contains_key(&key)
    }

    fn len(&self) -> usize {
        BTreeMap::len(self)
    }
}

/// Marker for storable component types.
pub trait Component: Any + 'static {}

/// Blanket impl: any `'static` type is a valid component.
impl<T: Any + 'static> Component for T {}

/// The ECS world: entities + component storages.
#[derive(Default)]
pub struct World {
    slots: Vec<u32>,
    free: Vec<u32>,
    storages: BTreeMap<TypeId, Box<dyn StorageAny>>,
}

impl World {
    /// New empty world.
    #[must_use]
    pub fn new() -> Self {
        World::default()
    }

    /// Spawn an entity, recycling slots with a bumped generation.
    pub fn spawn(&mut self) -> Entity {
        if let Some(index) = self.free.pop() {
            let generation = match self.slots.get(index as usize) {
                Some(&g) => g.wrapping_add(1),
                None => 1,
            };
            if let Some(slot) = self.slots.get_mut(index as usize) {
                *slot = generation;
            }
            return Entity { index, generation };
        }
        let index = u32::try_from(self.slots.len()).unwrap_or(u32::MAX);
        self.slots.push(0);
        Entity {
            index,
            generation: 0,
        }
    }

    /// Whether `entity` is alive.
    #[must_use]
    pub fn is_alive(&self, entity: Entity) -> bool {
        self.slots
            .get(entity.index as usize)
            .is_some_and(|&g| g == entity.generation)
    }

    /// Despawn an entity, dropping all its components. Returns
    /// whether the handle was alive.
    pub fn despawn(&mut self, entity: Entity) -> bool {
        if !self.is_alive(entity) {
            return false;
        }
        if let Some(slot) = self.slots.get_mut(entity.index as usize) {
            *slot = slot.wrapping_add(1);
        }
        let key = entity.key();
        for storage in self.storages.values_mut() {
            storage.remove(key);
        }
        self.free.push(entity.index);
        true
    }

    /// Number of alive entities.
    #[must_use]
    pub fn entity_count(&self) -> usize {
        self.slots.len().saturating_sub(self.free.len())
    }

    fn storage<C: Component>(&self) -> Option<&BTreeMap<u64, C>> {
        self.storages
            .get(&TypeId::of::<C>())
            .and_then(|s| s.as_any().downcast_ref::<BTreeMap<u64, C>>())
    }

    fn storage_mut<C: Component>(&mut self) -> &mut BTreeMap<u64, C> {
        let entry = self
            .storages
            .entry(TypeId::of::<C>())
            .or_insert_with(|| Box::new(BTreeMap::<u64, C>::new()));
        entry
            .as_any_mut()
            .downcast_mut::<BTreeMap<u64, C>>()
            .unwrap_or_else(|| {
                // The or_insert above always creates the matching
                // type; the empty map is unreachable in practice.
                Box::leak(Box::new(BTreeMap::new()))
            })
    }

    /// Attach component `c` to `entity` (replacing any prior).
    /// Returns false when the entity is dead.
    pub fn add<C: Component>(&mut self, entity: Entity, c: C) -> bool {
        if !self.is_alive(entity) {
            return false;
        }
        self.storage_mut::<C>().insert(entity.key(), c);
        true
    }

    /// Read a component.
    #[must_use]
    pub fn get<C: Component>(&self, entity: Entity) -> Option<&C> {
        if !self.is_alive(entity) {
            return None;
        }
        self.storage::<C>().and_then(|s| s.get(&entity.key()))
    }

    /// Mutably borrow a component.
    pub fn get_mut<C: Component>(&mut self, entity: Entity) -> Option<&mut C> {
        if !self.is_alive(entity) {
            return None;
        }
        self.storage_mut::<C>().get_mut(&entity.key())
    }

    /// Remove a component; true when it was present.
    pub fn remove<C: Component>(&mut self, entity: Entity) -> bool {
        if !self.is_alive(entity) {
            return false;
        }
        self.storage_mut::<C>().remove(&entity.key()).is_some()
    }

    /// Whether `entity` carries component `C`.
    #[must_use]
    pub fn has<C: Component>(&self, entity: Entity) -> bool {
        let key = entity.key();
        self.is_alive(entity) && self.storage::<C>().is_some_and(|s| s.contains(key))
    }

    /// Number of live `C` components in the world.
    #[must_use]
    pub fn component_count<C: Component>(&self) -> usize {
        self.storage::<C>().map_or(0, StorageAny::len)
    }

    /// Iterate `(entity, &C)` pairs in entity order.
    pub fn query<C: Component>(&self) -> impl Iterator<Item = (Entity, &C)> {
        self.storage::<C>()
            .into_iter()
            .flat_map(|s| s.iter())
            .map(|(k, v)| (Entity::from_key(*k), v))
    }

    /// Iterate entities carrying both `A` and `B` in entity order.
    /// `A` and `B` must be distinct types.
    pub fn query2<A: Component, B: Component>(&self) -> Vec<(Entity, (&A, &B))> {
        let mut out = Vec::new();
        for (e, a) in self.query::<A>() {
            if let Some(b) = self.get::<B>(e) {
                out.push((e, (a, b)));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq)]
    struct Position {
        x: f64,
        y: f64,
    }

    #[derive(Clone, Copy, Debug, PartialEq)]
    struct Velocity {
        dx: f64,
        dy: f64,
    }

    #[derive(Clone, Copy, Debug, PartialEq)]
    struct Tag;

    #[test]
    fn spawn_add_get_round_trip() {
        let mut w = World::new();
        let e = w.spawn();
        assert!(w.add(e, Position { x: 1.0, y: 2.0 }));
        assert_eq!(w.get::<Position>(e), Some(&Position { x: 1.0, y: 2.0 }));
        if let Some(p) = w.get_mut::<Position>(e) {
            p.x = 5.0;
        }
        assert_eq!(w.get::<Position>(e).map(|p| p.x), Some(5.0));
    }

    #[test]
    fn despawn_invalidates_and_cleans() {
        let mut w = World::new();
        let e = w.spawn();
        let _ = w.add(e, Position { x: 0.0, y: 0.0 });
        let _ = w.add(e, Tag);
        assert!(w.despawn(e), "despawn alive entity");
        assert!(!w.is_alive(e));
        assert!(w.get::<Position>(e).is_none(), "components dropped");
        assert!(!w.despawn(e), "double despawn is a no-op");
        assert_eq!(w.entity_count(), 0);
    }

    #[test]
    fn generation_blocks_stale_handles() {
        let mut w = World::new();
        let e1 = w.spawn();
        let _ = w.add(e1, Position { x: 9.0, y: 9.0 });
        let _ = w.despawn(e1);
        let e2 = w.spawn(); // recycles the slot, new generation
        assert_ne!(e1, e2, "generations differ");
        assert!(w.get::<Position>(e2).is_none(), "stale component invisible");
        // e1's handle can no longer read or write.
        assert!(!w.is_alive(e1));
        assert!(!w.add(e1, Position { x: 0.0, y: 0.0 }));
    }

    #[test]
    fn queries_filter_by_component_presence() {
        let mut w = World::new();
        let a = w.spawn();
        let b = w.spawn();
        let c = w.spawn();
        let _ = w.add(a, Position { x: 1.0, y: 1.0 });
        let _ = w.add(a, Velocity { dx: 1.0, dy: 0.0 });
        let _ = w.add(b, Position { x: 2.0, y: 2.0 });
        let _ = w.add(c, Velocity { dx: 0.0, dy: 1.0 });
        let movers: Vec<Entity> = w.query::<Velocity>().map(|(e, _)| e).collect();
        assert_eq!(movers, vec![a, c], "entity order, both movers");
        let both = w.query2::<Position, Velocity>();
        assert_eq!(both.len(), 1, "only `a` has both");
        assert_eq!(both.first().map(|(e, _)| *e), Some(a));
    }

    #[test]
    fn system_step_is_deterministic() {
        let run = || {
            let mut w = World::new();
            for k in 0..8_u32 {
                let e = w.spawn();
                let _ = w.add(
                    e,
                    Position {
                        x: f64::from(k),
                        y: 0.0,
                    },
                );
                let _ = w.add(e, Velocity { dx: 1.0, dy: 0.5 });
            }
            for _ in 0..30 {
                // Collect then write (immutable pass, mutable pass):
                // the standard two-phase system shape.
                let moves: Vec<(Entity, Velocity)> =
                    w.query::<Velocity>().map(|(e, v)| (e, *v)).collect();
                for (e, v) in moves {
                    if let Some(p) = w.get_mut::<Position>(e) {
                        p.x += v.dx * 0.016;
                        p.y += v.dy * 0.016;
                    }
                }
            }
            w.query::<Position>()
                .map(|(e, p)| (e.key(), (p.x, p.y)))
                .collect::<Vec<_>>()
        };
        assert_eq!(run(), run(), "system loop byte-identical across runs");
    }

    #[test]
    fn remove_component_only() {
        let mut w = World::new();
        let e = w.spawn();
        let _ = w.add(e, Position { x: 0.0, y: 0.0 });
        let _ = w.add(e, Tag);
        assert!(w.remove::<Tag>(e));
        assert!(w.get::<Tag>(e).is_none());
        assert!(w.get::<Position>(e).is_some(), "other components intact");
        assert!(!w.remove::<Tag>(e), "second remove is a no-op");
    }

    #[test]
    fn dead_entity_writes_are_rejected() {
        let mut w = World::new();
        let e = w.spawn();
        let _ = w.despawn(e);
        assert!(!w.add(e, Tag), "add on dead entity rejected");
        assert!(w.get::<Tag>(e).is_none());
        assert!(!w.remove::<Tag>(e));
    }
}
