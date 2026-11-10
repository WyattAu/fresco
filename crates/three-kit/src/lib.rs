//! 3D foundation for the fresco game-engine stack.
//!
//! [`math`] provides vectors, matrices, and projection; [`raster`]
//! provides a depth-buffered software triangle rasterizer over
//! paint-kit surfaces. Foundation increment: meshes, lighting, and
//! texture mapping build on this.

pub mod math;
pub mod mesh;
pub mod raster;

pub use math::{Mat4, Vec3};
pub use mesh::{Light, Mesh};
pub use raster::{Frame, Triangle, Vertex};
