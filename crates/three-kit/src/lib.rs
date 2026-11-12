//! 3D foundation for the fresco game-engine stack.
//!
//! [`math`] provides vectors, matrices, and projection; [`raster`]
//! provides a depth-buffered software triangle rasterizer over
//! paint-kit surfaces. Foundation increment: meshes, lighting, and
//! texture mapping build on this.

pub mod animation;
pub mod math;
pub mod mesh;
pub mod obj;
pub mod raster;
pub mod shadow;
pub mod skin;

pub use math::{Mat4, Vec3};
pub use mesh::{Light, Mesh};
pub use obj::{ObjMesh, import_obj};
pub use raster::{Frame, TexturedTriangle, Triangle, Vertex};
pub use skin::{Channel, Joint, Skin};
