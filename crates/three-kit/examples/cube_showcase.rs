//! Headless 3D showcase: a rotating, lit, textured cube rendered
//! through the full three-kit pipeline into BMP frames.
//!
//! Visual verification artifact for the 3D stack — the same
//! pattern fx_showcase uses: run headless, write frames, review
//! the images on a screen. `CUBE_SHOWCASE_OUT` selects the output
//! directory (default `/tmp/opencode/cube-showcase`).

use paint_kit::{Image, Painter, Surface};
use path_kit::Vec2;
use three_kit::{Frame, Light, Mat4, Mesh, Vec3};

/// A 2x2 checker texture (16x16 px, 8px cells).
fn checker_texture() -> Image {
    let w = 16_usize;
    let h = 16_usize;
    let mut data = vec![0_u8; w * h * 4];
    for row in 0..h {
        for col in 0..w {
            let base = (row * w + col) * 4;
            let cell = ((col / 8) + (row / 8)) % 2;
            let bytes: [u8; 4] = if cell == 0 {
                [235, 90, 60, 255] // warm orange
            } else {
                [240, 230, 210, 255] // warm white
            };
            if let Some(slot) = data.get_mut(base..base + 4) {
                slot.copy_from_slice(&bytes);
            }
        }
    }
    Image::new(w, h, data).unwrap_or_else(|| Image {
        width: 1,
        height: 1,
        data: vec![255, 255, 255, 255],
    })
}

/// Solid color per-face palette for the flat-shaded variant.
const FACE_COLORS: [[u8; 4]; 6] = [
    [220, 80, 70, 255],   // +Z red
    [70, 130, 220, 255],  // -Z blue
    [80, 200, 120, 255],  // +X green
    [230, 200, 80, 255],  // -X yellow
    [180, 100, 220, 255], // +Y purple
    [90, 200, 200, 255],  // -Y teal
];

fn main() {
    let out_dir = std::env::var("CUBE_SHOWCASE_OUT")
        .unwrap_or_else(|_| "/tmp/opencode/cube-showcase".to_owned());
    let _ = std::fs::create_dir_all(&out_dir);

    let width = 320_usize;
    let height = 240_usize;
    let proj = Mat4::perspective(std::f64::consts::FRAC_PI_4, 1.0, 0.1, 100.0);
    let eye = Vec3::new(3.0, 2.5, 4.0);
    let view = Mat4::look_at(eye, Vec3::ZERO, Vec3::new(0.0, 1.0, 0.0));
    let vp = proj.mul(&view);
    let light = Light {
        direction: Vec3::new(0.45, 0.8, 0.35)
            .normalize()
            .unwrap_or(Vec3::new(0.0, 1.0, 0.0)),
        ambient: 0.3,
    };
    let texture = checker_texture();

    // The cube's six faces in the same order as Mesh::cube, each
    // mapped to its checker UV window.
    let cube = Mesh::cube(1.0);

    for frame_idx in 0..12_u32 {
        let angle = f64::from(frame_idx) * std::f64::consts::TAU / 12.0;
        let world = cube.transform(&Mat4::rotate_y(angle).mul(&Mat4::rotate_x(angle * 0.5)));
        let mut surface = Surface::new(width, height).unwrap_or_default();
        let mut painter = Painter::default();
        // Background fill (dark slate).
        painter.fill_rect(
            &mut surface,
            path_kit::Rect::from_size(Vec2::ZERO, Vec2::new(320.0, 240.0)),
            &paint_kit::Paint::from(paint_kit::Solid::premul([24, 26, 34, 255])),
        );
        let mut frame = Frame::new(width, height);

        // Flat-shaded pass with per-face colors.
        let mut tris: Vec<three_kit::Triangle> = Vec::new();
        for (face_idx, face) in world.faces.iter().enumerate() {
            let Some(a) = world.verts.get(face[0] as usize).copied() else {
                continue;
            };
            let Some(b) = world.verts.get(face[1] as usize).copied() else {
                continue;
            };
            let Some(c) = world.verts.get(face[2] as usize).copied() else {
                continue;
            };
            let Some(n) = world.face_normal(*face) else {
                continue;
            };
            let centroid = a.add(b).add(c).scale(1.0 / 3.0);
            if n.dot(eye.sub(centroid)) <= 0.0 {
                continue; // backface
            }
            let base = FACE_COLORS
                .get(face_idx / 2)
                .copied()
                .unwrap_or([200, 200, 200, 255]);
            tris.push(three_kit::Triangle {
                verts: [
                    three_kit::Vertex { pos: a },
                    three_kit::Vertex { pos: b },
                    three_kit::Vertex { pos: c },
                ],
                color: light.shade(n, base),
            });
        }
        frame.draw_triangles(&mut surface, &mut painter, &vp, &tris, 0.1);

        // Write the flat frame.
        let flat_path = format!("{out_dir}/cube-flat-{frame_idx:02}.bmp");
        let aref = render_conformance::SurfaceRef {
            data: surface.data(),
            width: surface.width(),
            height: surface.height(),
            stride: surface.width() * 4,
        };
        let _ = render_conformance::write_bmp(std::path::Path::new(&flat_path), &aref);

        // Textured pass: full checker on every face.
        let mut surface2 = Surface::new(width, height).unwrap_or_default();
        let mut painter2 = Painter::default();
        painter2.fill_rect(
            &mut surface2,
            path_kit::Rect::from_size(Vec2::ZERO, Vec2::new(320.0, 240.0)),
            &paint_kit::Paint::from(paint_kit::Solid::premul([24, 26, 34, 255])),
        );
        let mut frame2 = Frame::new(width, height);
        let mut quads: Vec<three_kit::TexturedTriangle> = Vec::new();
        // Every face maps the full image so the checker pattern is
        // visible on all visible faces (v flipped: image v=0 is the
        // top row).
        let uv_window = |_face_idx: usize| -> [(f64, f64); 4] {
            [(0.0, 1.0), (1.0, 1.0), (1.0, 0.0), (0.0, 0.0)]
        };
        for (face_idx, face) in world.faces.iter().enumerate() {
            let Some(a) = world.verts.get(face[0] as usize).copied() else {
                continue;
            };
            let Some(b) = world.verts.get(face[1] as usize).copied() else {
                continue;
            };
            let Some(c) = world.verts.get(face[2] as usize).copied() else {
                continue;
            };
            let Some(n) = world.face_normal(*face) else {
                continue;
            };
            let centroid = a.add(b).add(c).scale(1.0 / 3.0);
            if n.dot(eye.sub(centroid)) <= 0.0 {
                continue;
            }
            let win = uv_window(face_idx / 2);
            // Face corners a,b,c map to the window's first three
            // corners; keep winding consistent.
            quads.push(three_kit::TexturedTriangle {
                verts: [
                    three_kit::Vertex { pos: a },
                    three_kit::Vertex { pos: b },
                    three_kit::Vertex { pos: c },
                ],
                uvs: [win[0], win[1], win[2]],
            });
        }
        frame2.draw_textured(&mut surface2, &mut painter2, &vp, &quads, &texture, 0.1);
        let tex_path = format!("{out_dir}/cube-textured-{frame_idx:02}.bmp");
        let aref2 = render_conformance::SurfaceRef {
            data: surface2.data(),
            width: surface2.width(),
            height: surface2.height(),
            stride: surface2.width() * 4,
        };
        let _ = render_conformance::write_bmp(std::path::Path::new(&tex_path), &aref2);
    }
    println!("frames written to {out_dir}");
}
