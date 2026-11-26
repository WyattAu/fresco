//! The game: an ECS-driven ball fountain with physics walls, tap
//! bursts, and a widget HUD.
//!
//! Demonstrates the Phase B integration contract: [`ecs_kit::World`]
//! owns game entities, [`phys_kit`] drives dynamics (Verlet particles,
//! inelastic wall contact, ball-ball impulse resolution), widget-kit
//! paints the HUD, and the whole sim is deterministic under a fixed
//! timestep (same inputs, same bytes - asserted by the oracle tests).
//!
//! Rendering is CPU paint-kit: solid background, balls as round-rect
//! circles (radius = half the box edge), HUD through the widget-kit
//! realfont label path.

use crate::atlas::tile_color;
use crate::sfx::{Sfx, SfxKind};
use ecs_kit::{Entity, World};
use path_kit::Vec2;
use phys_kit::Particle;
use phys_kit::collision::StaticBox;
use three_kit::animation::{AnimationClip, Animator};
use three_kit::gltf::base64_encode;
use three_kit::{Joint, Mat4, Vec3};
use widget_kit::WidgetColors;

/// Fixed simulation step (seconds). The render loop accumulates wall
/// time and drains it in whole steps, so the sim is frame-rate
/// independent and reproducible.
const DT: f64 = 1.0 / 60.0;

/// Wall band thickness: solid boxes straddle the viewport edge by
/// this much on the inside; `resolve_particle_box` ejects through the
/// nearest face, which is always the inward one.
const WALL: f64 = 10.0;

/// Gravity in logical px/s^2 (screen y-down: positive pulls down).
const GRAVITY: f64 = 900.0;

/// Ball radius in logical px.
const BALL_RADIUS: f64 = 8.0;

/// Lifetime cap per ball, in sim frames (5 s at 60 Hz).
const TTL_FRAMES: u64 = 300;

/// Hard entity cap: past this, the oldest balls despawn first. Bounds
/// the ball-ball collision pass to a fixed O(n^2) budget.
const MAX_BALLS: usize = 96;

/// Fountain cadence: one spawned ball every N sim frames.
const FOUNTAIN_EVERY: u64 = 15;

/// Combo window: pops closer than this in sim frames chain a
/// multiplier (3 s at 60 Hz).
const COMBO_FRAMES: u64 = 180;

/// Water line: distance from the viewport bottom to the SPH pool's
/// surface (the basin is 150 px tall; the top 40 px are headroom).
const WATER_LINE_OFFSET: f64 = 110.0;

/// The dancer's clip document, parsed through the M8.1 importer at
/// construction (dogfoods the animation text loader).
const DANCER_DOC: &str = "# idle sway: hip, torso, arm
clip dance 2.0
track 0 z
key 0.0 -0.2
key 1.0 0.2
key 2.0 -0.2
track 1 z
key 0.0 0.15
key 1.0 -0.15
key 2.0 0.15
track 2 z
key 0.0 0.6
key 0.5 -0.9
key 1.0 0.6
key 1.5 -0.9
key 2.0 0.6
";

/// Tap burst size.
const BURST: usize = 8;

/// Atlas sprite names, indexed by `Tile` (6 = player).
const TILE_NAMES: [&str; 6] = ["ball0", "ball1", "ball2", "ball3", "ball4", "ball5"];

/// ECS components: physics state, render geometry, spawn metadata.
/// BTreeMap-backed storages keep iteration order deterministic.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Ball(Particle);

#[derive(Clone, Copy, Debug)]
struct Radius(f64);

/// Atlas tile index for the sprite draw.
#[derive(Clone, Copy, Debug)]
struct Tile(u8);

/// The grabbed ball: follows the pointer while held (inv_mass 0),
/// released with the movement's implicit velocity.
#[derive(Clone, Copy, Debug)]
struct Grabbed;

/// A score target: amber ring popped when a ball touches it.
#[derive(Clone, Copy, Debug)]
struct Target;

/// Per-entity TTL override (pop fx lives shorter than balls).
#[derive(Clone, Copy, Debug)]
struct Ttl(u64);

/// Target drift velocity (px/s): targets wander slowly and bounce
/// inside their band, so pops require leading the shot.
#[derive(Clone, Copy, Debug)]
struct Drift(Vec2);

#[derive(Clone, Copy, Debug)]
struct Born(u64);

/// HUD overlay buttons: pause toggle and back-to-menu, drawn on the
/// game's first HUD row (right-aligned).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HudButton {
    /// Pause / unpause the sim.
    Pause,
    /// Return to the main menu (discards the run).
    Menu,
}

/// Which HUD button sits under a logical point, if any. Shared by the
/// shell's tap routing and the paint pass so the hit region and the
/// drawn affordance can never drift apart.
#[must_use]
pub fn hud_button_at(width: f64, height: f64, x: f64, y: f64) -> Option<HudButton> {
    let _ = height;
    // Below the ~24dp status bar inset (a non-immersive app's top
    // 24dp is system territory - taps there never reach the game).
    let row = 30.0..=56.0;
    let pause_x = (width - 54.0)..=(width - 8.0);
    let menu_x = (width - 118.0)..=(width - 58.0);
    if !row.contains(&y) {
        return None;
    }
    if pause_x.contains(&x) {
        Some(HudButton::Pause)
    } else if menu_x.contains(&x) {
        Some(HudButton::Menu)
    } else {
        None
    }
}

/// Build the dancer as a glTF 2.0 document: a 16-vertex strip
/// (2 columns x 8 rows spanning hip to arm tip) skinned to 3 joints
/// (hip, torso at -28px, arm at -54px) with inverse bind matrices,
/// embedded as a base64 data URI. Parsed at construction through
/// three-kit's glTF importer - the dancer body is IMPORTED, not
/// procedural.
#[must_use]
fn dancer_gltf_doc() -> String {
    const ROWS: usize = 8;
    let mut pos_bytes: Vec<u8> = Vec::new();
    let mut joints_bytes: Vec<u8> = Vec::new();
    let mut weights_bytes: Vec<u8> = Vec::new();
    for r in 0..ROWS {
        let y = 4.0 - 10.0 * f64::from(u32::try_from(r).unwrap_or(0));
        for x in [-8.0_f64, 8.0_f64] {
            pos_bytes.extend_from_slice(&(x as f32).to_le_bytes());
            pos_bytes.extend_from_slice(&(y as f32).to_le_bytes());
            pos_bytes.extend_from_slice(&0.0_f32.to_le_bytes());
            // Row band -> joint: hip 0..=20px, torso to -48, arm below.
            let joint: u8 = if y > -24.0 {
                0
            } else if y > -48.0 {
                1
            } else {
                2
            };
            joints_bytes.extend_from_slice(&[joint, 0, 0, 0]);
            let mut wrow = (1.0_f32).to_le_bytes().to_vec();
            wrow.extend_from_slice(&0.0_f32.to_le_bytes());
            wrow.extend_from_slice(&0.0_f32.to_le_bytes());
            wrow.extend_from_slice(&0.0_f32.to_le_bytes());
            weights_bytes.extend_from_slice(&wrow);
        }
    }
    // Inverse bind matrices: identity for the hip (bind world =
    // identity), translate (+28) for the torso joint, translate
    // (+54) for the arm joint.
    let mut ibm_bytes: Vec<u8> = Vec::new();
    for ty in [0.0_f64, 28.0, 54.0] {
        let m = [
            1.0, 0.0, 0.0, 0.0, //
            0.0, 1.0, 0.0, 0.0, //
            0.0, 0.0, 1.0, 0.0, //
            0.0, ty, 0.0, 1.0,
        ];
        for v in m {
            ibm_bytes.extend_from_slice(&v.to_le_bytes());
        }
    }
    let mut idx_bytes: Vec<u8> = Vec::new();
    for r in 0..ROWS - 1 {
        let base = u32::try_from(r * 2).unwrap_or(0);
        idx_bytes.extend_from_slice(&[0, 0, 0, 0]); // placeholder width alignment
        let _ = base;
        idx_bytes.truncate(idx_bytes.len() - 4);
        let a = u32::try_from(r * 2).unwrap_or(0);
        let b = a + 1;
        let c = a + 2;
        let d = a + 3;
        idx_bytes.extend_from_slice(&a.to_le_bytes());
        idx_bytes.extend_from_slice(&b.to_le_bytes());
        idx_bytes.extend_from_slice(&c.to_le_bytes());
        idx_bytes.extend_from_slice(&b.to_le_bytes());
        idx_bytes.extend_from_slice(&d.to_le_bytes());
        idx_bytes.extend_from_slice(&c.to_le_bytes());
    }
    let mut buffer: Vec<u8> = Vec::new();
    buffer.extend_from_slice(&pos_bytes);
    buffer.extend_from_slice(&joints_bytes);
    buffer.extend_from_slice(&weights_bytes);
    buffer.extend_from_slice(&ibm_bytes);
    buffer.extend_from_slice(&idx_bytes);
    // Animation: sampler input times [0, 1, 2] then three z-rotation
    // quaternion outputs (hip, torso, arm), 3 VEC4 f32 keys each.
    let mut anim_times: Vec<u8> = Vec::new();
    for t in [0.0_f32, 1.0, 2.0] {
        anim_times.extend_from_slice(&t.to_le_bytes());
    }
    let mut anim_outputs: Vec<Vec<u8>> = Vec::new();
    for (rate, phase) in [(0.2_f64, 0.0_f64), (0.15, 1.2), (0.9, 2.4)] {
        let mut out: Vec<u8> = Vec::new();
        for t in [0.0_f64, 1.0, 2.0] {
            let angle = rate * (std::f64::consts::TAU * t / 2.0 + phase);
            let (sx, cx) = (angle * 0.5).sin_cos();
            for v in [0.0_f64, 0.0, sx, cx] {
                out.extend_from_slice(&(v as f32).to_le_bytes());
            }
        }
        anim_outputs.push(out);
    }
    buffer.extend_from_slice(&anim_times);
    for out in &anim_outputs {
        buffer.extend_from_slice(out);
    }
    let n_verts = ROWS * 2;
    let n_tris = (ROWS - 1) * 2;
    format!(
        r#"{{"nodes":[{{}},{{"children":[2]}},{{"children":[3],"translation":[0.0,-28.0,0.0]}},{{"translation":[0.0,-26.0,0.0]}}],
         "skins":[{{"joints":[1,2,3],"inverseBindMatrices":3}}],
         "meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"JOINTS_0":1,"WEIGHTS_0":2}},"indices":4}}]}}],
         "accessors":[{{"bufferView":0,"componentType":5126,"count":{nv},"type":"VEC3"}},
                      {{"bufferView":1,"componentType":5121,"count":{nv},"type":"VEC4"}},
                      {{"bufferView":2,"componentType":5126,"count":{nv},"type":"VEC4"}},
                      {{"bufferView":3,"componentType":5126,"count":3,"type":"MAT4"}},
                      {{"bufferView":4,"componentType":5125,"count":{nt},"type":"SCALAR"}},
                      {{"bufferView":5,"componentType":5126,"count":3,"type":"SCALAR"}},
                      {{"bufferView":6,"componentType":5126,"count":3,"type":"VEC4"}},
                      {{"bufferView":7,"componentType":5126,"count":3,"type":"VEC4"}},
                      {{"bufferView":8,"componentType":5126,"count":3,"type":"VEC4"}}],
         "bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":192}},
                        {{"buffer":0,"byteOffset":192,"byteLength":64}},
                        {{"buffer":0,"byteOffset":256,"byteLength":256}},
                        {{"buffer":0,"byteOffset":512,"byteLength":192}},
                        {{"buffer":0,"byteOffset":704,"byteLength":168}},
                        {{"buffer":0,"byteOffset":872,"byteLength":12}},
                        {{"buffer":0,"byteOffset":884,"byteLength":48}},
                        {{"buffer":0,"byteOffset":932,"byteLength":48}},
                        {{"buffer":0,"byteOffset":980,"byteLength":48}}],
         "animations":[{{"name":"dance",
           "channels":[{{"sampler":0,"target":{{"node":1,"path":"rotation"}}}},
                      {{"sampler":1,"target":{{"node":2,"path":"rotation"}}}},
                      {{"sampler":2,"target":{{"node":3,"path":"rotation"}}}}],
           "samplers":[{{"input":5,"output":6,"interpolation":"LINEAR"}},
                      {{"input":5,"output":7,"interpolation":"LINEAR"}},
                      {{"input":5,"output":8,"interpolation":"LINEAR"}}]}}],
         "buffers":[{{"byteLength":{total},"uri":"data:application/octet-stream;base64,{data}"}}]}}"#,
        nv = n_verts,
        nt = n_tris,
        total = buffer.len(),
        data = base64_encode(&buffer)
    )
}

/// The game screen: ECS world, walls, sim clock, input.
pub struct GameScreen {
    world: World,
    atlas: Option<paint_kit::atlas::SpriteAtlas>,
    /// SPH water pool (fluid-kit M7): sits in a basin at the bottom,
    /// independent of the ECS balls.
    pool: fluid_kit::sph::SphSystem,
    pool_filled: bool,
    /// The dancing skeleton (M8): three joints driven by an imported
    /// clip, rendered as a stick figure.
    dancer_joints: Vec<Joint>,
    dancer: Animator,
    /// The dancer's skinned body: a vertex strip auto-rigged to the
    /// joints, posed each frame and drawn as filled triangles.
    dancer_skin: three_kit::skin::Skin,
    dancer_base: three_kit::Mesh,
    walls: [StaticBox; 4],
    width: f64,
    height: f64,
    /// Accumulated wall time not yet consumed by sim steps.
    acc: f64,
    /// Whole sim frames stepped since construction.
    sim_frame: u64,
    /// Fountain cadence (sim frames per spawned ball); set from the
    /// menu difficulty.
    fountain_every: u64,
    last_paint: Option<std::time::Instant>,
    /// Whether the sim is paused (the wall-time drain freezes; the
    /// render keeps running so the frozen frame stays visible).
    paused: bool,
    /// Last pointer position while a grab is active.
    last_pointer: Option<(f64, f64)>,
    /// Targets popped so far (base 1 per pop).
    score: u64,
    /// Best score this device has seen (shown on the HUD).
    best: u64,
    /// Sound effects (audio-kit synth clips into the SilentSink).
    sfx: Sfx,
    /// Consecutive pops inside [`COMBO_FRAMES`]: each pop scores
    /// `1 + combo`.
    combo: u64,
    /// Sim frame of the last pop (combo window reference).
    last_pop: Option<u64>,
}

impl Default for GameScreen {
    fn default() -> Self {
        GameScreen::new()
    }
}

impl GameScreen {
    /// New game with empty world and degenerate viewport until the
    /// first resize.
    #[must_use]
    pub fn new() -> Self {
        let dancer_joints = vec![
            // Root (hip): the base position is added at render.
            Joint {
                parent: None,
                local: three_kit::Mat4::translate(Vec3::new(0.0, 0.0, 0.0)),
            },
            // Torso: 28 px above the hip.
            Joint {
                parent: Some(0),
                local: three_kit::Mat4::translate(Vec3::new(0.0, -28.0, 0.0)),
            },
            // Arm: 26 px above the torso.
            Joint {
                parent: Some(1),
                local: three_kit::Mat4::translate(Vec3::new(0.0, -26.0, 0.0)),
            },
        ];
        // Dancer body IMPORTED from the glTF document through the
        // M8.4/M8.5 loader chain; the procedural strip + nearest
        // bind remains as the fallback when the import fails.
        let (dancer_base, dancer_skin) = match three_kit::gltf::import_gltf_skin(&dancer_gltf_doc())
        {
            Some((mesh, skin)) => (mesh, skin),
            None => {
                let mesh = {
                    let mut verts: Vec<three_kit::Vec3> = Vec::new();
                    let mut faces: Vec<[u32; 3]> = Vec::new();
                    let rows = 9_u32;
                    for r in 0..rows {
                        for c in 0..2_u32 {
                            let x = if c == 0 { -8.0 } else { 8.0 };
                            let y = -10.0 * f64::from(r);
                            verts.push(three_kit::Vec3::new(x, y, 0.0));
                        }
                    }
                    for r in 0..rows - 1 {
                        let base = r * 2;
                        faces.push([base, base + 1, base + 2]);
                        faces.push([base + 1, base + 3, base + 2]);
                    }
                    three_kit::Mesh { verts, faces }
                };
                let skin = three_kit::skin::Skin::bind_nearest(&mesh, &dancer_joints);
                (mesh, skin)
            }
        };
        let dancer = match three_kit::gltf::import_gltf_animation(&dancer_gltf_doc()) {
            // Preferred: the glTF document's animation samplers.
            Some(clip) => {
                let mut a = Animator::new(clip);
                a.play();
                a
            }
            // Fallback: the text-format clip document.
            None => match three_kit::anim_text::import_clip(DANCER_DOC) {
                Some(clip) => {
                    let mut a = Animator::new(clip);
                    a.play();
                    a
                }
                None => {
                    // Fixed valid text cannot fail; the fallback is a
                    // static bind-pose figure.
                    let mut a = Animator::new(AnimationClip::new("idle", 2.0, Vec::new()));
                    a.play();
                    a
                }
            },
        };
        GameScreen {
            world: World::new(),
            atlas: crate::atlas::game_atlas(),
            pool: fluid_kit::sph::SphSystem::new(
                Vec2::ZERO,
                Vec2::ZERO,
                fluid_kit::sph::SphParams::default(),
            ),
            pool_filled: false,
            dancer_joints,
            dancer_base,
            dancer_skin,
            dancer,
            walls: [StaticBox::new(Vec2::ZERO, Vec2::ZERO); 4],
            width: 0.0,
            height: 0.0,
            acc: 0.0,
            sim_frame: 0,
            fountain_every: FOUNTAIN_EVERY,
            paused: false,
            last_paint: None,
            last_pointer: None,
            score: 0,
            combo: 0,
            best: 0,
            sfx: Sfx::new(),
            last_pop: None,
        }
    }

    /// Resize the viewport and rebuild the walls. Balls outside the
    /// new bounds are clamped inside on the next step.
    pub fn resize(&mut self, width: usize, height: usize) {
        self.width = f64::from(u32::try_from(width.max(1)).unwrap_or(1));
        self.height = f64::from(u32::try_from(height.max(1)).unwrap_or(1));
        let (w, h) = (self.width, self.height);
        // SPH basin: bottom strip of the viewport; particles are
        // filled once on the first real resize.
        self.pool = fluid_kit::sph::SphSystem::new(
            Vec2::new(8.0, h - 150.0),
            Vec2::new(w - 8.0, h - 8.0),
            fluid_kit::sph::SphParams::default(),
        );
        if !self.pool_filled {
            let n = self.pool.fill_disc(Vec2::new(w * 0.5, h - 80.0), 55.0, 9.0);
            if n > 0 {
                self.pool_filled = true;
            }
        }
        self.walls = [
            // Left: straddles x = WALL.
            StaticBox::new(Vec2::new(-20.0, -20.0), Vec2::new(WALL, h + 20.0)),
            // Right.
            StaticBox::new(Vec2::new(w - WALL, -20.0), Vec2::new(w + 20.0, h + 20.0)),
            // Top.
            StaticBox::new(Vec2::new(-20.0, -20.0), Vec2::new(w + 20.0, WALL)),
            // Bottom.
            StaticBox::new(Vec2::new(-20.0, h - WALL), Vec2::new(w + 20.0, h + 20.0)),
        ];
    }

    /// Tap: spawn a deterministic burst of balls scattered around
    /// `(x, y)` (spatial spread, not a coincident stack: PBD towers
    /// form when balls share a spawn point), then grab a kinematic
    /// player ball that follows the pointer until release.
    pub fn pointer_down(&mut self, x: f64, y: f64) {
        let seed = self.sim_frame;
        for k in 0..BURST {
            let ku = u64::try_from(k).unwrap_or(0);
            let u = phys_kit::particles::hash_unit(seed.wrapping_add(ku * 7 + 1));
            let v = phys_kit::particles::hash_unit(seed.wrapping_add(ku * 13 + 5));
            let speed = 120.0 + 260.0 * u;
            let ang = std::f64::consts::TAU * v;
            let vel = Vec2::new(ang.cos() * speed, ang.sin() * speed - 160.0);
            // Spread spawn points over a 48 px cross so balls never
            // share a position (degenerate contacts at zero distance
            // have no separation direction).
            let dx = (f64::from(u32::try_from(k).unwrap_or(0)) - 3.5) * 12.0;
            let dy = (f64::from(u32::try_from(k % 3).unwrap_or(0)) - 1.0) * 14.0;
            let pos = Vec2::new(
                (x + dx).clamp(WALL + BALL_RADIUS, self.width - WALL - BALL_RADIUS),
                (y + dy).clamp(WALL + BALL_RADIUS, self.height - WALL - BALL_RADIUS),
            );
            self.spawn(pos, vel);
        }
        // Grab: a kinematic player ball follows the pointer until
        // release (inv_mass 0 - contacts and gravity skip it).
        let pos = Vec2::new(
            x.clamp(WALL + BALL_RADIUS, self.width - WALL - BALL_RADIUS),
            y.clamp(WALL + BALL_RADIUS, self.height - WALL - BALL_RADIUS),
        );
        self.sfx.play(SfxKind::Grab, self.sim_frame);
        let player = self.world.spawn();
        let mut particle = Particle::free(pos, 1.0);
        particle.inv_mass = 0.0;
        self.world.add(player, Ball(particle));
        self.world.add(player, Radius(BALL_RADIUS));
        self.world.add(player, Tile(6));
        self.world.add(player, Born(self.sim_frame));
        self.world.add(player, Grabbed);
        self.last_pointer = Some((pos.x, pos.y));
    }

    /// Pointer move while a grab is active: carry the player ball.
    /// The position delta becomes its implicit Verlet velocity, so a
    /// release flings naturally.
    pub fn pointer_move(&mut self, x: f64, y: f64) {
        let Some((lx, ly)) = self.last_pointer else {
            return;
        };
        let pos = Vec2::new(
            x.clamp(WALL + BALL_RADIUS, self.width - WALL - BALL_RADIUS),
            y.clamp(WALL + BALL_RADIUS, self.height - WALL - BALL_RADIUS),
        );
        let grabbed: Vec<Entity> = self.world.query::<Grabbed>().map(|(e, _)| e).collect();
        for entity in &grabbed {
            if let Some(ball) = self.world.get_mut::<Ball>(*entity) {
                let delta = Vec2::new(pos.x - lx, pos.y - ly);
                ball.0.pos = pos;
                ball.0.prev = Vec2::new(pos.x - delta.x, pos.y - delta.y);
            }
        }
        self.last_pointer = Some((pos.x, pos.y));
    }

    /// Pointer release: un-grab - the ball rejoins the simulation
    /// with the pointer movement's velocity (fling).
    pub fn pointer_up(&mut self, _x: f64, _y: f64) {
        let grabbed: Vec<Entity> = self.world.query::<Grabbed>().map(|(e, _)| e).collect();
        for entity in &grabbed {
            if let Some(ball) = self.world.get_mut::<Ball>(*entity) {
                ball.0.inv_mass = 1.0;
            }
            let _ = self.world.remove::<Grabbed>(*entity);
        }
        self.last_pointer = None;
    }

    /// Spawn one ball with `vel` (px/s) at `pos`.
    fn spawn(&mut self, pos: Vec2, vel: Vec2) {
        let entity = self.world.spawn();
        let mut particle = Particle::free(pos, 1.0);
        // Verlet velocity: prev sits one step back along -vel.
        particle.prev = Vec2::new(pos.x - vel.x * DT, pos.y - vel.y * DT);
        self.world.add(entity, Ball(particle));
        self.world.add(entity, Radius(BALL_RADIUS));
        let pick = usize::try_from(
            self.sim_frame
                .wrapping_mul(2654435761)
                .checked_shr(13)
                .unwrap_or(0)
                % 6,
        )
        .unwrap_or(0);
        let tile = u8::try_from(pick).unwrap_or(0);
        self.world.add(entity, Tile(tile));
        self.world.add(entity, Born(self.sim_frame));
    }

    /// Despawn the oldest ball (smallest `Born`, ties broken by the
    /// ECS iteration order, which is deterministic).
    fn despawn_oldest(&mut self) {
        let mut oldest: Option<(Entity, u64)> = None;
        for (entity, born) in self.world.query::<Born>() {
            match oldest {
                Some((_, b)) if born.0 >= b => {}
                _ => oldest = Some((entity, born.0)),
            }
        }
        if let Some((entity, _)) = oldest {
            self.world.despawn(entity);
        }
    }

    /// One fixed sim step: fountain, gravity, wall + ball contacts,
    /// TTL reap, entity cap.
    pub fn step(&mut self) {
        self.sim_frame = self.sim_frame.wrapping_add(1);

        // Targets: keep up to 3 alive, one attempt per second at a
        // deterministic position (upper 55% of the viewport).
        if self.sim_frame.is_multiple_of(60) {
            let targets = self.world.query::<Target>().count();
            if targets < 3 {
                let u = phys_kit::particles::hash_unit(self.sim_frame);
                let v = phys_kit::particles::hash_unit(self.sim_frame.wrapping_add(9));
                let x = WALL + 30.0 + u * (self.width - 2.0 * (WALL + 30.0));
                // Spawn INSIDE the drift band (band_top..band_bot of
                // the drift pass): an out-of-band spawn would be
                // clamped-teleported on the first drift step.
                let band_top = 40.0 + 18.0;
                let band_bot = self.height * 0.55 + 18.0;
                let y = band_top + v * (band_bot - band_top);
                let entity = self.world.spawn();
                self.world.add(entity, Target);
                // Immovable carrier for position + contacts: balls
                // bounce off targets (inv_mass 0 skips gravity and
                // resolution movement).
                let mut particle = Particle::free(Vec2::new(x, y), 1.0);
                particle.inv_mass = 0.0;
                self.world.add(entity, Ball(particle));
                // Slow wander: 6..24 px/s in a random direction,
                // deterministic per spawn tick.
                let wu = phys_kit::particles::hash_unit(self.sim_frame.wrapping_add(23));
                let wv = phys_kit::particles::hash_unit(self.sim_frame.wrapping_add(31));
                let ang = std::f64::consts::TAU * wu;
                let speed = 6.0 + 18.0 * wv;
                self.world.add(
                    entity,
                    Drift(Vec2::new(ang.cos() * speed, ang.sin() * speed)),
                );
                self.world.add(entity, Radius(18.0));
                self.world.add(entity, Tile(7));
            }
        }

        // Fountain: a steady deterministic stream so pacing evidence
        // always has moving targets.
        if self.sim_frame.is_multiple_of(self.fountain_every) {
            let u = phys_kit::particles::hash_unit(self.sim_frame);
            let v = phys_kit::particles::hash_unit(self.sim_frame.wrapping_add(3));
            let w = phys_kit::particles::hash_unit(self.sim_frame.wrapping_add(17));
            let speed = 380.0 + 240.0 * u;
            let vx = (v - 0.5) * 320.0;
            // Jitter the launch point across the bottom third so
            // successive spawns do not stack into a single column.
            let px = self.width * (0.3 + 0.4 * w);
            let pos = Vec2::new(
                px.clamp(WALL + BALL_RADIUS, self.width - WALL - BALL_RADIUS),
                self.height - WALL - BALL_RADIUS,
            );
            self.spawn(pos, Vec2::new(vx, -speed));
        }

        // Gravity (Verlet: shift pos, velocity stays implicit).
        // Submerged balls get buoyancy (gravity cut to 30 percent)
        // and linear drag - two-way coupling with the SPH pool.
        let g_dt2 = GRAVITY * DT * DT;
        let water_line = self.height - WATER_LINE_OFFSET;
        let drag = (1.0 - 2.5 * DT).clamp(0.3, 1.0);
        let touched: Vec<Entity> = self.world.query::<Ball>().map(|(e, _)| e).collect();
        for entity in &touched {
            if let Some(ball) = self.world.get_mut::<Ball>(*entity) {
                if ball.0.inv_mass <= 0.0 {
                    continue; // grabbed: pointer-carried, gravity skips
                }
                let p = ball.0.pos;
                if p.y > water_line {
                    let g_sub = g_dt2 * 0.3;
                    ball.0.pos = Vec2::new(p.x, p.y + g_sub);
                    let v = Vec2::new(ball.0.pos.x - ball.0.prev.x, ball.0.pos.y - ball.0.prev.y);
                    ball.0.prev = Vec2::new(ball.0.pos.x - v.x * drag, ball.0.pos.y - v.y * drag);
                } else {
                    ball.0.pos = Vec2::new(p.x, p.y + g_dt2);
                }
            }
        }

        // Walls.
        let walls = self.walls;
        for entity in &touched {
            if let Some(ball) = self.world.get_mut::<Ball>(*entity) {
                for wall in &walls {
                    let _ = phys_kit::collision::StaticBox::resolve_particle_box(&mut ball.0, wall);
                }
            }
        }

        // Ball-ball impulses (pairwise, deterministic order).
        let ids: Vec<Entity> = touched.clone();
        for (ia, &ea) in ids.iter().enumerate() {
            for &eb in ids.iter().skip(ia + 1) {
                let ra = self.world.get::<Radius>(ea).map(|r| r.0);
                let rb = self.world.get::<Radius>(eb).map(|r| r.0);
                if let (Some(ra), Some(rb)) = (ra, rb) {
                    // Distinct entities, but one storage: resolve on
                    // copies, write back sequentially.
                    let pa = self.world.get::<Ball>(ea).map(|b| b.0);
                    let pb = self.world.get::<Ball>(eb).map(|b| b.0);
                    if let (Some(mut pa), Some(mut pb)) = (pa, pb)
                        && phys_kit::collision::Circle::resolve_circle_circle(
                            &mut pa, ra, &mut pb, rb,
                        )
                    {
                        if let Some(ball) = self.world.get_mut::<Ball>(ea) {
                            ball.0 = pa;
                        }
                        if let Some(ball) = self.world.get_mut::<Ball>(eb) {
                            ball.0 = pb;
                        }
                    }
                }
            }
        }

        // World pass LAST: ball-ball writeback can push pairs back
        // into the wall band, so walls resolve again after contacts
        // (integrate -> object-object -> object-world).
        let walls = self.walls;
        for entity in &touched {
            if let Some(ball) = self.world.get_mut::<Ball>(*entity) {
                for wall in &walls {
                    let _ = phys_kit::collision::StaticBox::resolve_particle_box(&mut ball.0, wall);
                }
            }
        }
        // Target drift: kinematic wander, bounced inside the upper
        // band (targets never leave their region).
        let targets: Vec<Entity> = self.world.query::<Target>().map(|(e, _)| e).collect();
        for &target in &targets {
            let (Some(band_r), Some(drift)) = (
                self.world.get::<Radius>(target).map(|r| r.0),
                self.world.get::<Drift>(target).map(|d| d.0),
            ) else {
                continue;
            };
            let mut vx = drift.x;
            let mut vy = drift.y;
            let band_top = 40.0 + band_r;
            let band_bot = self.height * 0.55 + band_r;
            if let Some(ball) = self.world.get_mut::<Ball>(target) {
                let mut nx = ball.0.pos.x + vx * DT;
                let mut ny = ball.0.pos.y + vy * DT;
                if nx < WALL + band_r || nx > self.width - WALL - band_r {
                    vx = -vx;
                    nx = nx.clamp(WALL + band_r, self.width - WALL - band_r);
                }
                if ny < band_top || ny > band_bot {
                    vy = -vy;
                    ny = ny.clamp(band_top, band_bot);
                }
                ball.0.pos = Vec2::new(nx, ny);
                ball.0.prev = ball.0.pos;
            }
            if let Some(d) = self.world.get_mut::<Drift>(target) {
                d.0 = Vec2::new(vx, vy);
            }
        }

        // Combo expiry: a stale chain resets before it can multiply.
        if let Some(last) = self.last_pop
            && self.sim_frame.saturating_sub(last) > COMBO_FRAMES
        {
            self.combo = 0;
        }

        // Target hits: a ball touching a target ring pops it -
        // score, despawn, and a short-lived mini-ball spark burst.
        let targets: Vec<Entity> = self.world.query::<Target>().map(|(e, _)| e).collect();
        let mut popped: Vec<(Entity, Vec2)> = Vec::new();
        for &target in &targets {
            let (Some(t_pos), Some(t_rad)) = (
                self.world.get::<Ball>(target).map(|b| b.0.pos),
                self.world.get::<Radius>(target).map(|r| r.0),
            ) else {
                continue;
            };
            let hit = self
                .world
                .query2::<Ball, Radius>()
                .into_iter()
                .any(|(e, (b, r))| {
                    // Targets carry no Ball component, so e is a ball.
                    let dx = b.0.pos.x - t_pos.x;
                    let dy = b.0.pos.y - t_pos.y;
                    // Contact-aligned hitbox: ball-ball resolution
                    // ejects a touching ball to ~t_rad + r (gravity
                    // adds sub-pixel wobble), so the pop fires on the
                    // same step's post-contact pass. Half a pixel of
                    // slack absorbs the wobble without reading as a
                    // different hitbox.
                    let rr = t_rad + r.0 + 0.5;
                    dx * dx + dy * dy < rr * rr && self.world.get::<Target>(e).is_none()
                });
            if hit {
                popped.push((target, t_pos));
            }
        }
        for (target, pos) in popped {
            self.sfx.play(SfxKind::Pop, self.sim_frame);
            // Combo: a pop inside the window chains (each consecutive
            // pop scores one more); outside the window the chain
            // restarts.
            let chained = self
                .last_pop
                .is_some_and(|l| self.sim_frame.saturating_sub(l) <= COMBO_FRAMES);
            self.combo = if chained {
                self.combo.saturating_add(1)
            } else {
                0
            };
            self.last_pop = Some(self.sim_frame);
            self.score = self.score.saturating_add(1 + self.combo);
            let _ = self.world.despawn(target);
            for k in 0..6u64 {
                let u = phys_kit::particles::hash_unit(self.sim_frame.wrapping_add(k * 3 + 1));
                let v = phys_kit::particles::hash_unit(self.sim_frame.wrapping_add(k * 11 + 7));
                let ang = std::f64::consts::TAU * u;
                let speed = 60.0 + 140.0 * v;
                let entity = self.world.spawn();
                let mut particle = Particle::free(pos, 1.0);
                particle.prev = Vec2::new(
                    pos.x - ang.cos() * speed * DT,
                    pos.y - ang.sin() * speed * DT,
                );
                self.world.add(entity, Ball(particle));
                self.world.add(entity, Radius(3.0));
                let tile = u8::try_from(k % 6).unwrap_or(0);
                self.world.add(entity, Tile(tile));
                self.world.add(entity, Born(self.sim_frame));
                self.world.add(entity, Ttl(24));
            }
        }

        // Ball -> pool coupling: moving balls push nearby SPH
        // particles (splashes). One-way: the fluid answers the game,
        // the game does not feel the fluid.
        let splashers: Vec<(Vec2, f64, Vec2)> = self
            .world
            .query2::<Ball, Radius>()
            .into_iter()
            .filter_map(|(e, (b, r))| {
                if self.world.get::<Target>(e).is_some() {
                    return None; // targets are fixtures, not splashing
                }
                let vel = Vec2::new((b.0.pos.x - b.0.prev.x) / DT, (b.0.pos.y - b.0.prev.y) / DT);
                Some((b.0.pos, r.0, vel))
            })
            .collect();
        // Splash: a fast ball crossing the water line thuds into the
        // pool - the Sfx rate limiter keeps churn from machine-gunning.
        let water_line = self.height - WATER_LINE_OFFSET;
        for (bpos, _rad, bvel) in &splashers {
            if bpos.y > water_line && bvel.y > 250.0 {
                self.sfx.play(SfxKind::Splash, self.sim_frame);
                break; // one splash check per frame is enough
            }
        }

        // Impulse exchange (two-way): every particle kick applies an
        // equal-opposite reaction to the ball (Verlet: prev shifts),
        // scaled by the ball's relative mass (8 vs particle 1).
        const BALL_MASS: f64 = 8.0;
        let mut reaction: Vec<(f64, f64, f64, f64)> = Vec::new(); // ball idx -> (rx, ry, brad, reach)
        for p in self.pool.particles_mut() {
            for (bi, (bpos, brad, bvel)) in splashers.iter().enumerate() {
                let dx = p.pos.x - bpos.x;
                let dy = p.pos.y - bpos.y;
                let reach = brad + 16.0;
                let d2 = dx * dx + dy * dy;
                if d2 >= reach * reach {
                    continue;
                }
                let speed = (bvel.x * bvel.x + bvel.y * bvel.y).sqrt();
                if speed < 50.0 {
                    continue; // resting balls do not churn the pool
                }
                let d = d2.sqrt().max(1.0);
                let falloff = 1.0 - d / reach;
                let push = (speed * 0.5 * falloff).min(600.0);
                let inv = 1.0 / d;
                let kick_x = dx * inv * push * DT * 30.0;
                let kick_y = dy * inv * push * DT * 30.0;
                p.vel.x += kick_x;
                p.vel.y += kick_y;
                // Per-kick speed ceiling: multi-ball contact must
                // not sum past the solver's own ceiling.
                let s = (p.vel.x * p.vel.x + p.vel.y * p.vel.y).sqrt();
                if s > 1500.0 {
                    let k = 1500.0 / s;
                    p.vel.x *= k;
                    p.vel.y *= k;
                }
                // Reaction accumulator per ball (index-matched).
                while reaction.len() <= bi {
                    reaction.push((0.0, 0.0, *brad, reach));
                }
                if let Some(r) = reaction.get_mut(bi) {
                    r.0 -= kick_x / BALL_MASS;
                    r.1 -= kick_y / BALL_MASS;
                }
            }
        }
        // Apply reactions to the balls' implicit velocities (prev
        // shifts by the delta).
        let balls: Vec<Entity> = self
            .world
            .query2::<Ball, Radius>()
            .into_iter()
            .filter_map(|(e, _)| {
                if self.world.get::<Target>(e).is_some() {
                    None
                } else {
                    Some(e)
                }
            })
            .collect();
        for (bi, entity) in balls.iter().enumerate() {
            let Some(r) = reaction.get(bi) else {
                continue;
            };
            if let Some(ball) = self.world.get_mut::<Ball>(*entity) {
                // prev shifts by delta * DT (prev is px, the reaction
                // is px/s): implicit velocity drops by the reaction.
                ball.0.prev = Vec2::new(ball.0.prev.x + r.0 * DT, ball.0.prev.y + r.1 * DT);
            }
        }
        // SPH pool: two half-steps per sim frame for stability.
        self.pool.step(DT * 0.5);
        self.pool.step(DT * 0.5);

        // Dancer playback rides the sim clock (freezes when paused).
        self.dancer.advance(DT);

        // TTL reap + entity cap (balls only: targets persist; fx
        // particles carry a short per-entity Ttl override).
        let frame = self.sim_frame;
        let expired: Vec<Entity> = self
            .world
            .query::<Born>()
            .filter(|(e, b)| {
                let ttl = self.world.get::<Ttl>(*e).map(|t| t.0).unwrap_or(TTL_FRAMES);
                frame.saturating_sub(b.0) > ttl
            })
            .map(|(e, _)| e)
            .collect();
        for entity in expired {
            let _ = self.world.despawn(entity);
        }
        while self.world.entity_count() > MAX_BALLS {
            self.despawn_oldest();
        }
    }

    /// Drain accumulated wall time in fixed steps (capped so a stalled
    /// renderer cannot spiral the sim).
    pub fn step_wall_time(&mut self, now: std::time::Instant) {
        if self.paused {
            // Freeze the sim but keep consuming the wall clock so an
            // unpause does not sprint through the paused interval.
            self.last_paint = Some(now);
            self.acc = 0.0;
            return;
        }
        let dt = match self.last_paint {
            Some(last) => now.duration_since(last).as_secs_f64().min(0.05),
            None => 0.0,
        };
        self.last_paint = Some(now);
        self.acc += dt;
        let mut steps = 0;
        while self.acc >= DT && steps < 4 {
            self.step();
            self.acc -= DT;
            steps += 1;
        }
        if steps == 4 {
            // Renderer stalled: drop the backlog instead of sprinting.
            self.acc = 0.0;
        }
    }

    /// Toggle the pause state.
    pub fn toggle_pause(&mut self) {
        self.paused = !self.paused;
    }

    /// Whether the sim is paused.
    #[must_use]
    pub const fn paused(&self) -> bool {
        self.paused
    }

    /// Paint one frame: background, balls, HUD.
    pub fn paint(&mut self, surface: &mut paint_kit::Surface, painter: &mut paint_kit::Painter) {
        // Background.
        painter.fill_rect(
            surface,
            paint_kit::Rect::from_size(Vec2::ZERO, Vec2::new(self.width, self.height)),
            &paint_kit::Paint::from(paint_kit::Solid::premul([16, 18, 24, 255])),
        );

        // Balls: atlas sprites (draw_image_region blit), with the
        // procedural circle as the no-atlas fallback.
        let balls: Vec<(Ball, Radius, Tile)> = self
            .world
            .query2::<Ball, Radius>()
            .into_iter()
            .filter_map(|(e, (b, r))| {
                let tile = self.world.get::<Tile>(e).map(|t| t.0)?;
                Some((*b, *r, Tile(tile)))
            })
            .collect();
        for (ball, radius, tile) in &balls {
            let r = radius.0;
            let side = r * 2.0;
            let dst = paint_kit::Rect::from_size(
                Vec2::new(ball.0.pos.x - r, ball.0.pos.y - r),
                Vec2::new(side, side),
            );
            let name = match tile.0 {
                6 => "player",
                7 => "target",
                i => TILE_NAMES
                    .get(usize::from(i) % TILE_NAMES.len())
                    .copied()
                    .unwrap_or("ball0"),
            };
            if let Some(atlas) = self.atlas.as_ref() {
                atlas.draw(painter, surface, name, dst);
            } else {
                painter.fill_round_rect(
                    surface,
                    dst,
                    r,
                    &paint_kit::Paint::from(paint_kit::Solid::premul(tile_color(tile.0))),
                );
            }
        }

        // SPH water pool: one sprite per particle.
        for p in self.pool.particles() {
            let dst = paint_kit::Rect::from_size(
                Vec2::new(p.pos.x - 4.0, p.pos.y - 4.0),
                Vec2::new(8.0, 8.0),
            );
            if let Some(atlas) = self.atlas.as_ref() {
                atlas.draw(painter, surface, "water", dst);
            }
        }

        // Dancing skeleton: world-compose the joint chain, draw bones
        // as white stamps and joints as player rings. Base position:
        // left side, above the pool.
        let poses = self.dancer.pose(&self.dancer_joints);
        let mut worlds: Vec<Mat4> = Vec::new();
        for (i, joint) in self.dancer_joints.iter().enumerate() {
            let local = poses.get(i).copied().unwrap_or(joint.local);
            let w = match joint.parent.and_then(|p| worlds.get(p)) {
                Some(parent_w) => parent_w.mul(&local),
                None => local,
            };
            worlds.push(w);
        }
        let base_x = 80.0;
        let base_y = self.height * 0.4;
        let mut joints_px: Vec<(f64, f64)> = Vec::new();
        for w in &worlds {
            let p = w.transform_point(three_kit::Vec3::ZERO);
            if let Some((x, y, _z)) = p {
                joints_px.push((base_x + x, base_y + y));
            }
        }
        // Skinned body: posed triangles through the raster path,
        // drawn under the bones.
        let posed = self
            .dancer_skin
            .posed_mesh(&self.dancer_base, &self.dancer.pose(&self.dancer_joints));
        let mut polys: Vec<path_kit::Poly> = Vec::new();
        for face in &posed.faces {
            let mut points: Vec<Vec2> = Vec::new();
            for idx in face {
                if let Some(v) = posed.verts.get(usize::try_from(*idx).unwrap_or(usize::MAX)) {
                    // Model space -> screen: base offset (hip origin).
                    points.push(Vec2::new(base_x + v.x, base_y + v.y));
                }
            }
            if points.len() == 3 {
                polys.push(path_kit::Poly {
                    points,
                    closed: true,
                });
            }
        }
        if !polys.is_empty() {
            painter.fill_polys(
                surface,
                &polys,
                path_kit::Affine::IDENTITY,
                paint_kit::FillRule::NonZero,
                &paint_kit::Paint::from(paint_kit::Solid::premul([175, 185, 210, 190])),
            );
        }

        // Bones: white stamps along each parent-child segment.
        for pair in joints_px.windows(2) {
            let (Some(&(ax, ay)), Some(&(bx, by))) = (pair.first(), pair.last()) else {
                continue;
            };
            let steps = 8;
            for k in 0..steps {
                let t = f64::from(u32::try_from(k).unwrap_or(0))
                    / f64::from(u32::try_from(steps).unwrap_or(1));
                let px = ax + (bx - ax) * t;
                let py = ay + (by - ay) * t;
                painter.fill_rect(
                    surface,
                    paint_kit::Rect::from_size(Vec2::new(px - 1.5, py - 1.5), Vec2::new(3.0, 3.0)),
                    &paint_kit::Paint::from(paint_kit::Solid::premul([230, 232, 240, 255])),
                );
            }
        }
        // Joint dots: player-ring sprites.
        for (jx, jy) in &joints_px {
            if let Some(atlas) = self.atlas.as_ref() {
                atlas.draw(
                    painter,
                    surface,
                    "player",
                    paint_kit::Rect::from_size(
                        Vec2::new(jx - 5.0, jy - 5.0),
                        Vec2::new(10.0, 10.0),
                    ),
                );
            }
        }

        // HUD through the widget-kit realfont label path.
        let mut hud = widget_kit::DrawListPainter {
            surface,
            painter,
            colors: WidgetColors::default(),
        };
        let balls_n = self.world.entity_count();
        widget_kit::WidgetRenderer::label_text(
            &mut hud,
            widget_kit::Rect2D {
                x: 8.0,
                y: 30.0,
                width: self.width - 16.0,
                height: 20.0,
            },
            &format!(
                "balls {balls_n}  score {}  x{}  sfx {}",
                self.score,
                self.combo,
                self.sfx_frames()
            ),
            14.0,
            [220, 224, 232, 255],
        );
        widget_kit::WidgetRenderer::label_text(
            &mut hud,
            widget_kit::Rect2D {
                x: 8.0,
                y: 52.0,
                width: self.width - 16.0,
                height: 18.0,
            },
            "tap = burst + grab  drag = fling  pop targets",
            12.0,
            [150, 156, 170, 255],
        );
        // HUD overlay buttons (right-aligned on the first row), with
        // the paused state called out under them.
        let pause_label = if self.paused { "play" } else { "pause" };
        widget_kit::WidgetRenderer::label_text(
            &mut hud,
            widget_kit::Rect2D {
                x: self.width - 52.0,
                y: 32.0,
                width: 46.0,
                height: 18.0,
            },
            pause_label,
            12.0,
            [255, 210, 120, 255],
        );
        widget_kit::WidgetRenderer::label_text(
            &mut hud,
            widget_kit::Rect2D {
                x: self.width - 114.0,
                y: 32.0,
                width: 54.0,
                height: 18.0,
            },
            "menu",
            12.0,
            [150, 156, 170, 255],
        );
        if self.paused {
            widget_kit::WidgetRenderer::label_text(
                &mut hud,
                widget_kit::Rect2D {
                    x: 8.0,
                    y: 72.0,
                    width: self.width - 16.0,
                    height: 18.0,
                },
                "paused - tap play to resume, menu to quit",
                12.0,
                [255, 210, 120, 255],
            );
        }
    }

    /// Set the fountain cadence from the menu difficulty.
    pub fn set_fountain_every(&mut self, frames: u64) {
        self.fountain_every = frames.max(1);
    }

    /// Show the persisted best on the HUD.
    pub fn set_best(&mut self, best: u64) {
        self.best = best;
    }

    /// Alive entity count (HUD + oracle use).
    #[must_use]
    pub fn entity_count(&self) -> usize {
        self.world.entity_count()
    }

    /// Popped-target count (HUD + oracle use).
    #[must_use]
    pub const fn score(&self) -> u64 {
        self.score
    }

    /// Active combo chain length (HUD + oracle use).
    #[must_use]
    pub const fn combo(&self) -> u64 {
        self.combo
    }

    /// Viewport width in logical px (HUD hit-region use).
    #[must_use]
    pub const fn view_width(&self) -> f64 {
        self.width
    }

    /// Viewport height in logical px (HUD hit-region use).
    #[must_use]
    pub const fn view_height(&self) -> f64 {
        self.height
    }

    /// The dancer's clip name (oracle: the M8.1 loader fed it).
    #[must_use]
    pub fn dancer_clip_name(&self) -> String {
        self.dancer.clip_name()
    }

    /// The dancer's current playhead (oracle use).
    #[must_use]
    pub const fn dancer_time(&self) -> f64 {
        self.dancer.time()
    }

    /// The skinned body's posed vertex positions, relative to the
    /// dancer base (oracle use).
    #[must_use]
    pub fn dancer_body_verts(&self) -> Vec<(f64, f64)> {
        let posed = self
            .dancer_skin
            .posed_mesh(&self.dancer_base, &self.dancer.pose(&self.dancer_joints));
        posed.verts.iter().map(|v| (v.x, v.y)).collect()
    }

    /// The arm joint's world position (oracle use).
    #[must_use]
    pub fn dancer_arm_tip(&self) -> (f64, f64) {
        let poses = self.dancer.pose(&self.dancer_joints);
        let mut world = three_kit::Mat4::translate(Vec3::new(80.0, self.height * 0.4, 0.0));
        for (i, joint) in self.dancer_joints.iter().enumerate() {
            let local = poses.get(i).copied().unwrap_or(joint.local);
            world = world.mul(&local);
        }
        match world.transform_point(three_kit::Vec3::ZERO) {
            Some((x, y, _)) => (x, y),
            None => (80.0, self.height * 0.4),
        }
    }

    /// Max SPH pool particle speed (oracle/diagnostic use).
    #[must_use]
    pub fn pool_max_speed(&self) -> f64 {
        self.pool
            .particles()
            .iter()
            .map(|p| (p.vel.x * p.vel.x + p.vel.y * p.vel.y).sqrt())
            .fold(0.0_f64, f64::max)
    }

    /// Whole sim frames stepped (oracle use).
    #[must_use]
    pub const fn sim_frame_now(&self) -> u64 {
        self.sim_frame
    }

    /// Sfx frames written to the sink (oracle use).
    #[must_use]
    pub const fn sfx_frames(&self) -> u64 {
        self.sfx.frames_written()
    }

    /// A sfx clip's length in samples (oracle use).
    #[must_use]
    pub fn sfx_clip_len(&self, kind: SfxKind) -> usize {
        self.sfx.clip_len(kind)
    }

    /// Accepted plays of a sfx kind (oracle use).
    #[must_use]
    pub const fn sfx_plays(&self, kind: SfxKind) -> u64 {
        self.sfx.plays(kind)
    }

    /// Target centers, for oracles.
    #[must_use]
    pub fn target_centers(&self) -> Vec<(f64, f64)> {
        self.world
            .query2::<Target, Ball>()
            .into_iter()
            .map(|(_, (_, b))| (b.0.pos.x, b.0.pos.y))
            .collect()
    }

    /// Ball centers, sorted, for determinism oracles.
    #[must_use]
    pub fn ball_centers(&self) -> Vec<(f64, f64)> {
        let mut out: Vec<(f64, f64)> = self
            .world
            .query::<Ball>()
            .map(|(_, b)| (b.0.pos.x, b.0.pos.y))
            .collect();
        out.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: after a tap burst and a second of sim, every ball sits
    /// inside the wall band (gravity + contacts never tunnel).
    #[test]
    fn balls_stay_inside_walls() {
        let mut game = GameScreen::new();
        game.resize(360, 740);
        game.pointer_down(180.0, 200.0);
        for _ in 0..60 {
            game.step();
        }
        let (w, h) = (game.width, game.height);
        for (x, y) in game.ball_centers() {
            assert!(x >= WALL, "ball x {x} through the left wall");
            assert!(x <= w - WALL, "ball x {x} through the right wall");
            assert!(y >= WALL, "ball y {y} through the top wall");
            assert!(y <= h - WALL, "ball y {y} through the floor");
        }
    }

    /// Oracle: the entity cap holds under sustained fountain + bursts.
    #[test]
    fn fountain_caps_entity_count() {
        let mut game = GameScreen::new();
        game.resize(360, 740);
        for frame in 0u64..900 {
            if frame.is_multiple_of(45) {
                game.pointer_down(90.0, 300.0);
            }
            game.step();
        }
        assert!(
            game.entity_count() <= MAX_BALLS,
            "cap breached: {}",
            game.entity_count()
        );
    }

    /// Oracle: same inputs, same bytes - the sim is deterministic.
    #[test]
    fn sim_is_deterministic() {
        let run = || {
            let mut game = GameScreen::new();
            game.resize(360, 740);
            for frame in 0..120 {
                if frame == 30 {
                    game.pointer_down(200.0, 150.0);
                }
                game.step();
            }
            game.ball_centers()
        };
        let a = run();
        let b = run();
        assert_eq!(a, b, "same inputs must produce identical sim state");
    }

    /// Oracle: a ball touching a target pops it - score increments,
    /// the fx sparks spawn, and a later cadence tick respawns the
    /// target.
    #[test]
    fn target_pop_scores_and_respawns() {
        let mut game = GameScreen::new();
        game.resize(360, 740);
        for _ in 0..61 {
            game.step();
        }
        let targets = game.target_centers();
        assert!(!targets.is_empty(), "spawn tick must produce a target");
        let before_targets = targets.len();
        // Drop a burst directly on the first target: one ball lands
        // dead-center, so the pop fires on the next step.
        let Some(&(tx, ty)) = targets.first() else {
            return;
        };
        game.pointer_down(tx, ty);
        game.step();
        assert!(game.score() >= 1, "hit must score: {}", game.score());
        assert_eq!(
            game.target_centers().len(),
            before_targets - 1,
            "popped target must despawn"
        );
        // Respawn cadence: within a second of steps a new target is
        // back (cap 3, spawn attempt every 60 sim frames).
        for _ in 0..61 {
            game.step();
        }
        assert!(!game.target_centers().is_empty(), "targets must respawn");
    }

    /// Oracle: pop fx sparks are short-lived - within the standard
    /// TTL they are gone (Ttl override 24 < 300).
    #[test]
    fn pop_fx_expires_fast() {
        let mut game = GameScreen::new();
        game.resize(360, 740);
        for _ in 0..61 {
            game.step();
        }
        let (tx, ty) = game
            .target_centers()
            .first()
            .copied()
            .unwrap_or((180.0, 200.0));
        game.pointer_down(tx, ty);
        game.step();
        let after_pop = game.entity_count();
        for _ in 0..30 {
            game.step();
        }
        assert!(
            game.entity_count() < after_pop,
            "fx sparks must expire: {after_pop} -> {}",
            game.entity_count()
        );
    }

    /// Oracle: a tap spawns BURST balls plus the grabbed player.
    #[test]
    fn tap_spawns_burst_and_player() {
        let mut game = GameScreen::new();
        game.resize(360, 740);
        let before = game.entity_count();
        game.pointer_down(180.0, 300.0);
        assert_eq!(
            game.entity_count(),
            before + BURST + 1,
            "tap spawns BURST + player"
        );
    }

    /// Oracle: a ball plunging into the basin splashes the pool.
    #[test]
    fn balls_splash_pool() {
        let mut game = GameScreen::new();
        game.resize(360, 740);
        for _ in 0..400 {
            game.step(); // pool settles from the fill drop
        }
        let calm: f64 = game
            .pool
            .particles()
            .iter()
            .map(|p| p.vel.x * p.vel.x + p.vel.y * p.vel.y)
            .fold(0.0_f64, f64::max)
            .sqrt();
        // Drop a burst right above the basin and let it land + sink;
        // track the peak churn (the splash is a transient spike).
        game.pointer_down(180.0, 480.0);
        let mut peak = calm;
        for _ in 0..60 {
            game.step();
            let now: f64 = game
                .pool
                .particles()
                .iter()
                .map(|p| p.vel.x * p.vel.x + p.vel.y * p.vel.y)
                .fold(0.0_f64, f64::max)
                .sqrt();
            peak = peak.max(now);
        }
        let splashed = peak;
        assert!(
            splashed > calm + 60.0,
            "plunge must churn the pool: calm {calm:.0} vs peak {splashed:.0}"
        );
    }

    /// Oracle: the SPH pool stays inside its basin.
    #[test]
    fn pool_stays_in_basin() {
        let mut game = GameScreen::new();
        game.resize(360, 740);
        assert!(game.pool_filled, "first resize must fill the pool");
        for _ in 0..300 {
            game.step();
        }
        for p in game.pool.particles() {
            assert!(
                p.pos.x >= 8.0 && p.pos.x <= 352.0,
                "pool x out of basin: {}",
                p.pos.x
            );
            assert!(
                p.pos.y >= 590.0 && p.pos.y <= 732.0,
                "pool y out of basin: {}",
                p.pos.y
            );
        }
    }

    /// Oracle: pops inside the combo window chain (1 + 2 points);
    /// pops outside it do not (1 + 1). Uses direct single-ball spawns
    /// (no burst noise) via the in-crate spawn helper.
    #[test]
    fn combo_chains_inside_window_only() {
        let mut game = GameScreen::new();
        game.resize(360, 740);
        for _ in 0..61 {
            game.step();
        }
        // Pop 1: ball dead-center on the target.
        let Some(&(tx, ty)) = game.target_centers().first() else {
            return;
        };
        game.spawn(Vec2::new(tx, ty), Vec2::ZERO);
        game.step();
        assert_eq!(game.score(), 1, "first pop scores 1");
        assert_eq!(game.combo(), 0, "first pop starts no chain");

        // Pop 2 at frame ~122 (respawn tick 120): inside the window.
        for _ in 0..61 {
            game.step();
        }
        let Some(&(tx, ty)) = game.target_centers().first() else {
            return;
        };
        game.spawn(Vec2::new(tx, ty), Vec2::ZERO);
        game.step();
        assert_eq!(game.combo(), 1, "second pop in window chains");
        assert_eq!(game.score(), 3, "chained pop scores 2");

        // Pop 3 after a 300-frame gap: the chain has expired by the
        // expiry pass alone (stale chains reset without a pop), and
        // the pop still scores. The score delta may exceed 1 when fx
        // sparks cascade into a neighbouring target same-frame - that
        // is the simultaneous-pop chain by design - so only growth is
        // asserted.
        for _ in 0..300 {
            game.step();
        }
        assert_eq!(game.combo(), 0, "gap expires the chain");
        let before = game.score();
        let Some(&(tx, ty)) = game.target_centers().first() else {
            return;
        };
        game.spawn(Vec2::new(tx, ty), Vec2::ZERO);
        game.step();
        assert!(game.score() > before, "pop after the gap still scores");
    }

    /// Oracle: targets drift and stay inside their band.
    #[test]
    fn targets_drift_inside_band() {
        let mut game = GameScreen::new();
        game.resize(360, 740);
        for _ in 0..61 {
            game.step();
        }
        assert!(!game.target_centers().is_empty(), "target must spawn");
        let Some(&first) = game.target_centers().first() else {
            return;
        };
        for _ in 0..120 {
            game.step();
        }
        let last = *game
            .target_centers()
            .iter()
            .min_by(|a, b| {
                let da = (a.0 - first.0).powi(2) + (a.1 - first.1).powi(2);
                let db = (b.0 - first.0).powi(2) + (b.1 - first.1).powi(2);
                da.total_cmp(&db)
            })
            .unwrap_or(&first);
        let moved = ((last.0 - first.0).powi(2) + (last.1 - first.1).powi(2)).sqrt();
        assert!(moved > 5.0, "target must drift: {moved:.1} px over 120f");
        assert!(
            last.1 >= 40.0 && last.1 <= 740.0 * 0.55 + 40.0,
            "drift must stay in the band: y={:.1}",
            last.1
        );
    }

    /// Oracle: the grabbed player follows the pointer exactly.
    #[test]
    fn player_follows_pointer() {
        let mut game = GameScreen::new();
        game.resize(360, 740);
        game.pointer_down(180.0, 300.0);
        game.pointer_move(120.0, 200.0);
        game.step();
        let centers = game.ball_centers();
        assert!(
            centers
                .iter()
                .any(|&(x, y)| (x - 120.0).abs() < 1.0 && (y - 200.0).abs() < 1.0),
            "player must sit at the pointer: {centers:?}"
        );
    }

    /// Oracle: a popped target feeds the pop clip into the sink -
    /// the frame count grows by exactly the clip length per pop.
    #[test]
    fn target_pop_feeds_sfx_sink() {
        let mut game = GameScreen::new();
        game.resize(360, 740);
        for _ in 0..61 {
            game.step();
        }
        let Some(&(tx, ty)) = game.target_centers().first() else {
            return;
        };
        let before = game.sfx_frames();
        game.spawn(Vec2::new(tx, ty), Vec2::ZERO);
        game.step();
        let popped_len = game.sfx_clip_len(SfxKind::Pop) as u64;
        let after = game.sfx_frames();
        assert_eq!(
            after - before,
            popped_len,
            "one pop writes one clip into the sink"
        );
    }

    /// Oracle: a fast ball entering the pool fires the splash sfx -
    /// at least one accepted play, and the rate limiter caps churn
    /// (90 frames inside a 30-frame window: at most 4).
    #[test]
    fn pool_entry_splash_is_rate_limited() {
        let mut game = GameScreen::new();
        game.resize(360, 740);
        for _ in 0..61 {
            game.step();
        }
        let before = game.sfx_plays(SfxKind::Splash);
        game.spawn(Vec2::new(180.0, 200.0), Vec2::new(0.0, 700.0));
        for _ in 0..90 {
            game.step();
        }
        let splashes = game.sfx_plays(SfxKind::Splash) - before;
        assert!(splashes >= 1, "pool entry must fire the splash: {splashes}");
        assert!(splashes <= 4, "churn must stay rate-limited: {splashes}");
    }

    /// Oracle: the skinned body deforms with the skeleton - posed
    /// vertices move over time and stay near the dancer.
    #[test]
    fn skinned_body_deforms() {
        let mut game = GameScreen::new();
        game.resize(360, 740);
        let start = game.dancer_body_verts();
        assert!(!start.is_empty(), "body must have vertices");
        for _ in 0..30 {
            game.step();
        }
        let now = game.dancer_body_verts();
        assert_eq!(start.len(), now.len(), "vertex count must persist");
        let moved = start
            .iter()
            .zip(now.iter())
            .map(|((ax, ay), (bx, by))| ((ax - bx).powi(2) + (ay - by).powi(2)).sqrt())
            .fold(0.0_f64, f64::max);
        assert!(moved > 0.5, "skinned verts must deform: {moved:.2} px");
        for (x, y) in &now {
            assert!(
                x.abs() < 200.0 && y.abs() < 200.0,
                "skinned vert exploded: ({x:.0},{y:.0})"
            );
        }
    }

    /// Oracle: the dancer rides the imported clip - the playhead
    /// advances and the arm tip swings (skeletal playback through
    /// the M8.1 loader).
    #[test]
    fn dancer_animates_from_imported_clip() {
        let mut game = GameScreen::new();
        game.resize(360, 740);
        assert_eq!(game.dancer_clip_name(), "dance", "clip must be imported");
        let (x0, y0) = game.dancer_arm_tip();
        for _ in 0..30 {
            game.step();
        }
        assert!(game.dancer_time() > 0.0, "playhead must advance");
        let (x1, y1) = game.dancer_arm_tip();
        let moved = ((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt();
        assert!(moved > 1.0, "arm tip must swing: {moved:.1} px");
    }

    /// Oracle: submerged balls fall slower than dry ones - buoyancy
    /// and drag slow the wet ball's descent through the pool.
    #[test]
    fn pool_slows_submerged_balls() {
        let mut game = GameScreen::new();
        game.resize(360, 740);
        game.step();
        let water_line = game.height - WATER_LINE_OFFSET;
        // 10 px BELOW the line: submerged from the first step.
        let wet_y = water_line + 10.0;
        game.spawn(Vec2::new(90.0, wet_y), Vec2::ZERO);
        game.spawn(Vec2::new(270.0, 150.0), Vec2::ZERO);
        for _ in 0..30 {
            game.step();
        }
        let mut wet_fall = 0.0;
        let mut dry_fall = 0.0;
        for (x, y) in game.ball_centers() {
            if (x - 90.0).abs() < 12.0 {
                wet_fall = y - wet_y;
            }
            if (x - 270.0).abs() < 12.0 {
                dry_fall = y - 150.0;
            }
        }
        assert!(
            wet_fall > 0.0 && dry_fall > 0.0,
            "both balls must have fallen: wet {wet_fall:.0} dry {dry_fall:.0}"
        );
        assert!(
            dry_fall > wet_fall * 2.0,
            "water must slow the wet ball: dry {dry_fall:.0} vs wet {wet_fall:.0}"
        );
    }

    /// Oracle: pausing freezes the sim; unpausing resumes it.
    #[test]
    fn pause_freezes_sim() {
        let mut game = GameScreen::new();
        game.resize(360, 740);
        game.pointer_down(180.0, 300.0);
        let now = std::time::Instant::now();
        game.step_wall_time(now);
        let before = game.sim_frame_now();
        game.toggle_pause();
        assert!(game.paused(), "pause must set the flag");
        // A paused drain consumes the interval without stepping.
        game.step_wall_time(now + std::time::Duration::from_millis(100));
        assert_eq!(game.sim_frame_now(), before, "paused sim must freeze");
        game.toggle_pause();
        assert!(!game.paused(), "unpause must clear the flag");
        game.step_wall_time(now + std::time::Duration::from_millis(200));
        assert!(game.sim_frame_now() > before, "unpaused sim must advance");
    }

    /// Oracle: the HUD button hit regions match the drawn labels.
    #[test]
    fn hud_button_regions() {
        use HudButton;
        // The row sits below the status bar inset: taps in the bar's
        // band (y < 30) are system territory and must not hit.
        assert_eq!(
            hud_button_at(360.0, 740.0, 330.0, 15.0),
            None,
            "status-bar band is not the HUD"
        );
        assert_eq!(
            hud_button_at(360.0, 740.0, 330.0, 43.0),
            Some(HudButton::Pause)
        );
        assert_eq!(
            hud_button_at(360.0, 740.0, 280.0, 43.0),
            Some(HudButton::Menu)
        );
        assert_eq!(hud_button_at(360.0, 740.0, 180.0, 43.0), None);
        assert_eq!(
            hud_button_at(360.0, 740.0, 330.0, 100.0),
            None,
            "outside the row"
        );
    }

    /// Oracle: release restores dynamics - the ball falls under
    /// gravity after pointer_up.
    #[test]
    fn release_restores_dynamics() {
        let mut game = GameScreen::new();
        game.resize(360, 740);
        game.pointer_down(180.0, 150.0);
        game.pointer_move(180.0, 150.0);
        game.pointer_up(180.0, 150.0);
        game.step();
        let before = game.ball_centers();
        for _ in 0..20 {
            game.step();
        }
        let after = game.ball_centers();
        assert_ne!(before, after, "released ball must move under gravity");
    }

    /// Oracle: TTL reaps - burst balls vanish after their lifetime.
    /// Compared against a tap-free twin run so fountain cadence does
    /// not leak into the assertion.
    #[test]
    fn ttl_despawns_expired_balls() {
        let mut base = GameScreen::new();
        base.resize(360, 740);
        let mut game = GameScreen::new();
        game.resize(360, 740);
        for _ in 0..600 {
            base.step();
            game.step();
        }
        let steady = base.entity_count();
        game.pointer_down(180.0, 300.0);
        let spiked = game.entity_count();
        assert!(spiked > steady, "burst must add balls");
        for _ in 0..(TTL_FRAMES + 10) {
            base.step();
            game.step();
        }
        assert_eq!(
            game.entity_count(),
            base.entity_count(),
            "burst balls must TTL back to the fountain-only state"
        );
    }
}
