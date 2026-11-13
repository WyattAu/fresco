//! 3D Eulerian fluid simulation for the fresco game-engine stack.
//!
//! Stable-fluids solver: semi-Lagrangian advection (unconditionally
//! stable), buoyancy-driven density, Jacobi-iterated pressure
//! projection to incompressibility. Deterministic: fixed cell
//! iteration order, pure arithmetic, no platform state.

pub mod grid;
pub mod slice;
pub mod solver;

pub use grid::{Field, VelocityField};
pub use solver::FluidSolver;
