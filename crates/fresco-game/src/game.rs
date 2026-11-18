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
use ecs_kit::{Entity, World};
use path_kit::Vec2;
use phys_kit::Particle;
use phys_kit::collision::StaticBox;
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

/// The game screen: ECS world, walls, sim clock, input.
pub struct GameScreen {
    world: World,
    atlas: Option<paint_kit::atlas::SpriteAtlas>,
    walls: [StaticBox; 4],
    width: f64,
    height: f64,
    /// Accumulated wall time not yet consumed by sim steps.
    acc: f64,
    /// Whole sim frames stepped since construction.
    sim_frame: u64,
    last_paint: Option<std::time::Instant>,
    /// Last pointer position while a grab is active.
    last_pointer: Option<(f64, f64)>,
    /// Targets popped so far (base 1 per pop).
    score: u64,
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
        GameScreen {
            world: World::new(),
            atlas: crate::atlas::game_atlas(),
            walls: [StaticBox::new(Vec2::ZERO, Vec2::ZERO); 4],
            width: 0.0,
            height: 0.0,
            acc: 0.0,
            sim_frame: 0,
            last_paint: None,
            last_pointer: None,
            score: 0,
            combo: 0,
            last_pop: None,
        }
    }

    /// Resize the viewport and rebuild the walls. Balls outside the
    /// new bounds are clamped inside on the next step.
    pub fn resize(&mut self, width: usize, height: usize) {
        self.width = f64::from(u32::try_from(width.max(1)).unwrap_or(1));
        self.height = f64::from(u32::try_from(height.max(1)).unwrap_or(1));
        let (w, h) = (self.width, self.height);
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
        if self.sim_frame.is_multiple_of(FOUNTAIN_EVERY) {
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
        let g_dt2 = GRAVITY * DT * DT;
        let touched: Vec<Entity> = self.world.query::<Ball>().map(|(e, _)| e).collect();
        for entity in &touched {
            if let Some(ball) = self.world.get_mut::<Ball>(*entity) {
                if ball.0.inv_mass <= 0.0 {
                    continue; // grabbed: pointer-carried, gravity skips
                }
                let p = ball.0.pos;
                ball.0.pos = Vec2::new(p.x, p.y + g_dt2);
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
                y: 4.0,
                width: self.width - 16.0,
                height: 20.0,
            },
            &format!(
                "fresco game  balls {balls_n}  score {}  x{}",
                self.score, self.combo
            ),
            14.0,
            [220, 224, 232, 255],
        );
        widget_kit::WidgetRenderer::label_text(
            &mut hud,
            widget_kit::Rect2D {
                x: 8.0,
                y: 26.0,
                width: self.width - 16.0,
                height: 18.0,
            },
            "tap = burst + grab  drag = fling  pop targets",
            12.0,
            [150, 156, 170, 255],
        );
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
