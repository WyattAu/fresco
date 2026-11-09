//! Undo/redo command stack for editing sessions.
//!
//! The CAD editing backbone: commands mutate document state and can
//! be inverted. The stack holds a bounded history; undo walks it
//! backwards, redo forwards. Commands are user-supplied closures
//! over an arbitrary `S` state plus an inverse factory, so the
//! stack stays generic (scene edits, property changes, geometry
//! operations all share one mechanism).

/// A reversible edit: apply the change and return the inverse
/// command capturing whatever the change destroyed.
pub trait Command<S> {
    /// Human-readable name for UI listing.
    fn name(&self) -> &str;
    /// Apply to `state`; returns the inverse command (boxed for
    /// type erasure).
    fn apply(&self, state: &mut S) -> Box<dyn Command<S>>;
    /// Re-run after undo; same as apply for stateless commands.
    fn reapply(&self, state: &mut S) -> Box<dyn Command<S>> {
        self.apply(state)
    }
}

/// Bounded undo/redo history over state `S`.
///
/// Invariant: after `apply`, `redo` is empty (a new edit invalidates
/// the redo branch). After `undo`, the undone command is pushed to
/// redo; after `redo`, it returns to undo.
pub struct History<S> {
    undo_stack: Vec<(String, Box<dyn Command<S>>)>,
    redo_stack: Vec<(String, Box<dyn Command<S>>)>,
    limit: usize,
}

impl<S: 'static> History<S> {
    /// New history holding at most `limit` commands (oldest
    /// discarded first).
    #[must_use]
    pub fn new(limit: usize) -> Self {
        History {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            limit: limit.max(1),
        }
    }

    /// Apply `cmd` to `state`, record it, clear the redo branch.
    pub fn apply(&mut self, state: &mut S, cmd: &dyn Command<S>) {
        let inverse = cmd.apply(state);
        self.undo_stack.push((cmd.name().to_owned(), inverse));
        self.redo_stack.clear();
        if self.undo_stack.len() > self.limit {
            self.undo_stack.remove(0);
        }
    }

    /// Undo the most recent command into `state`. Returns the
    /// command name, if there was anything to undo.
    pub fn undo(&mut self, state: &mut S) -> Option<String> {
        let (name, inverse) = self.undo_stack.pop()?;
        // Applying the inverse captures ITS inverse: the re-do
        // command. Push that, not the original inverse.
        let redone = inverse.apply(state);
        self.redo_stack.push((name.clone(), redone));
        Some(name)
    }

    /// Redo the most recently undone command into `state`.
    /// Returns the command name, if there was anything to redo.
    pub fn redo(&mut self, state: &mut S) -> Option<String> {
        let (name, cmd) = self.redo_stack.pop()?;
        let undone = cmd.reapply(state);
        self.undo_stack.push((name.clone(), undone));
        Some(name)
    }

    /// Number of undoable commands.
    #[must_use]
    pub fn undo_len(&self) -> usize {
        self.undo_stack.len()
    }

    /// Number of redoable commands.
    #[must_use]
    pub fn redo_len(&self) -> usize {
        self.redo_stack.len()
    }

    /// Drop all history.
    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }
}

/// Convenience: a value-swap command built from a getter/setter
/// pair. Apply sets `new`; the inverse restores whatever was there.
pub struct SetValue<S, V> {
    /// Command name.
    pub name: &'static str,
    /// Read the current value.
    pub get: fn(&S) -> V,
    /// Write a value.
    pub set: fn(&mut S, V),
    /// The value to set on apply.
    pub value: V,
}

impl<S: 'static, V> Command<S> for SetValue<S, V>
where
    V: Clone + PartialEq + 'static,
{
    fn name(&self) -> &str {
        self.name
    }

    fn apply(&self, state: &mut S) -> Box<dyn Command<S>> {
        let old = (self.get)(state);
        (self.set)(state, self.value.clone());
        Box::new(SetValue {
            name: self.name,
            get: self.get,
            set: self.set,
            value: old,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use path_kit::Vec2;

    /// Tiny document: a list of shape positions.
    #[derive(Default)]
    struct Doc {
        shapes: Vec<Vec2>,
    }

    /// Move shape `i` by `delta`.
    struct Move {
        index: usize,
        delta: Vec2,
    }

    impl Command<Doc> for Move {
        fn name(&self) -> &str {
            "move"
        }

        fn apply(&self, state: &mut Doc) -> Box<dyn Command<Doc>> {
            let Some(shape) = state.shapes.get(self.index) else {
                return Box::new(Noop);
            };
            let from = *shape;
            let to = Vec2::new(from.x + self.delta.x, from.y + self.delta.y);
            if let Some(slot) = state.shapes.get_mut(self.index) {
                *slot = to;
            }
            Box::new(Move {
                index: self.index,
                delta: Vec2::new(from.x - to.x, from.y - to.y),
            })
        }
    }

    struct Noop;

    impl Command<Doc> for Noop {
        fn name(&self) -> &str {
            "noop"
        }

        fn apply(&self, _state: &mut Doc) -> Box<dyn Command<Doc>> {
            Box::new(Noop)
        }
    }

    #[test]
    fn apply_undo_round_trips() {
        let mut doc = Doc {
            shapes: vec![Vec2::new(10.0, 10.0)],
        };
        let mut h = History::new(50);
        h.apply(
            &mut doc,
            &Move {
                index: 0,
                delta: Vec2::new(5.0, -3.0),
            },
        );
        assert_eq!(doc.shapes.first().copied(), Some(Vec2::new(15.0, 7.0)));
        let name = h.undo(&mut doc);
        assert_eq!(name.as_deref(), Some("move"));
        assert_eq!(doc.shapes.first().copied(), Some(Vec2::new(10.0, 10.0)));
    }

    #[test]
    fn undo_then_redo_reapplies() {
        let mut doc = Doc {
            shapes: vec![Vec2::new(0.0, 0.0)],
        };
        let mut h = History::new(50);
        h.apply(
            &mut doc,
            &Move {
                index: 0,
                delta: Vec2::new(7.0, 0.0),
            },
        );
        let _ = h.undo(&mut doc);
        let name = h.redo(&mut doc);
        assert_eq!(name.as_deref(), Some("move"));
        assert_eq!(doc.shapes.first().copied(), Some(Vec2::new(7.0, 0.0)));
        assert_eq!(h.redo_len(), 0, "redo branch consumed");
    }

    #[test]
    fn new_apply_clears_redo_branch() {
        let mut doc = Doc {
            shapes: vec![Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0)],
        };
        let mut h = History::new(50);
        h.apply(
            &mut doc,
            &Move {
                index: 0,
                delta: Vec2::new(2.0, 0.0),
            },
        );
        let _ = h.undo(&mut doc);
        assert_eq!(h.redo_len(), 1);
        h.apply(
            &mut doc,
            &Move {
                index: 1,
                delta: Vec2::new(3.0, 0.0),
            },
        );
        assert_eq!(h.redo_len(), 0, "fresh edit invalidates redo");
        assert_eq!(h.undo_len(), 1);
    }

    #[test]
    fn history_is_bounded() {
        let mut doc = Doc::default();
        let mut h = History::new(3);
        for k in 0..10_u32 {
            doc.shapes.push(Vec2::new(f64::from(k), 0.0));
        }
        for _ in 0..10 {
            h.apply(
                &mut doc,
                &Move {
                    index: 0,
                    delta: Vec2::new(1.0, 0.0),
                },
            );
        }
        assert_eq!(h.undo_len(), 3, "oldest commands discarded");
        // Three undos land on a well-defined state.
        for _ in 0..3 {
            let _ = h.undo(&mut doc);
        }
        assert_eq!(h.undo_len(), 0);
    }

    #[test]
    fn set_value_command_inverts() {
        struct Cfg {
            zoom: f64,
        }
        let mut cfg = Cfg { zoom: 1.0 };
        let mut h: History<Cfg> = History::new(20);
        h.apply(
            &mut cfg,
            &SetValue {
                name: "set zoom",
                get: |c: &Cfg| c.zoom,
                set: |c: &mut Cfg, v| c.zoom = v,
                value: 2.5_f64,
            },
        );
        assert!((cfg.zoom - 2.5).abs() < 1e-9);
        let _ = h.undo(&mut cfg);
        assert!((cfg.zoom - 1.0).abs() < 1e-9, "inverse restored 1.0");
        let _ = h.redo(&mut cfg);
        assert!((cfg.zoom - 2.5).abs() < 1e-9);
    }

    #[test]
    fn undo_on_empty_is_none() {
        let mut doc = Doc::default();
        let mut h = History::new(10);
        assert!(h.undo(&mut doc).is_none());
        assert!(h.redo(&mut doc).is_none());
    }
}
