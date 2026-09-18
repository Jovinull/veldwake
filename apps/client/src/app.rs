use std::{
    error::Error,
    fmt::{self, Display, Formatter},
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use tracing::{debug, info, warn};
use veldwake_streaming::{CacheConfig, ChunkCache, StreamingConfig};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{DeviceEvent, ElementState, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

use veldwake_character::GroundSampler;
use veldwake_combat::{CombatEvent, SIDES, Side};

use crate::{
    arena,
    camera::{Camera, CameraController, FollowController, HitShake},
    character::{CharacterScene, CharacterSelection, TerrainGround, spawn_character_camera},
    debug::{DebugMode, combat_primitives, debug_primitives},
    encounter::{EncounterMode, EncounterScene},
    input::{CameraAction, CombatAction, InputState},
    lighting::Weather,
    readout::{self, READOUT_INSTANCES},
    renderer::{RenderOutcome, Renderer, VfxFrameWork},
    streaming::{ChunkPresentation, FrameStreamingReport, StreamingBridge, UploadBudget},
    vfx::{MAX_VFX_INSTANCES, VfxInstance, VfxKind, VfxPool},
    world::{WorldSelection, requested_pose, resolve_pose},
};

const FRAME_REPORT_INTERVAL: Duration = Duration::from_secs(5);

/// Opt-in directory for the experimental M3D chunk cache. Unset means no
/// cache and no filesystem access from the streaming worker.
const CACHE_DIR_VARIABLE: &str = "VELDWAKE_CACHE_DIR";

/// How often the character's state is reported, in seconds.
///
/// The same five-second cadence the streaming report uses, so a capture
/// can be aligned with one interval line rather than with two clocks.
const CHARACTER_REPORT_INTERVAL: Duration = Duration::from_secs(5);

/// How often the encounter's state is reported, on the same cadence again.
const COMBAT_REPORT_INTERVAL: Duration = Duration::from_secs(5);

/// Where the camera starts.
///
/// `VELDWAKE_POSE` names either one of the world's golden terrain poses or
/// one of the character poses, which frame the body rather than the valley.
/// Character poses need the world's generator to know what the character is
/// standing on, so the two are resolved together here rather than in either
/// module alone.
fn spawn_camera() -> Camera {
    let world = WorldSelection::from_environment();
    let requested = requested_pose();
    if let Some(name) = requested.as_deref()
        && let Some(generator) = world.generator()
    {
        // A combat pose first: it frames the arena, and a frozen moment needs a
        // fixed camera rather than one that follows a body.
        if let Some(camera) = arena::spawn_combat_camera(name, &generator) {
            return camera;
        }
        if let Some(camera) = spawn_character_camera(name, &generator) {
            return camera;
        }
    }
    world.spawn_camera(requested.as_deref())
}

pub fn run() -> Result<(), AppRunError> {
    let event_loop = EventLoop::new().map_err(|error| AppRunError(error.to_string()))?;
    event_loop.set_control_flow(ControlFlow::Wait);

    let mut app = App::default();
    event_loop
        .run_app(&mut app)
        .map_err(|error| AppRunError(error.to_string()))?;

    if let Some(error) = app.fatal_error {
        return Err(AppRunError(error));
    }

    Ok(())
}

#[derive(Debug)]
pub struct AppRunError(String);

impl Display for AppRunError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for AppRunError {}

struct App {
    renderer: Option<Renderer>,
    streaming: Option<StreamingBridge>,
    camera: Camera,
    controller: CameraController,
    input: InputState,
    last_frame: Instant,
    frame_stats: FrameStats,
    occluded: bool,
    fatal_error: Option<String>,
    /// Active debug view, cycled by `F1`. `Off` is the shipped rendering.
    debug_mode: DebugMode,
    /// Whether the box-drawing views draw their boxes, toggled by `F2`.
    debug_boxes: bool,
    /// Current weather state, toggled by `F3`. A switch, not a simulation.
    weather: Weather,
    /// The terrain generator the character's ground query borrows from.
    /// `None` for the diagnostic corridor, which is not generated terrain.
    terrain: Option<veldwake_procedural::TerrainGenerator>,
    /// The character, its state, and the diagnostic course driving it.
    character: Option<CharacterScene>,
    last_character_report: Instant,
    /// The fight, its clock, and what drives it. `None` when the encounter is off.
    encounter: Option<EncounterScene>,
    /// The third-person camera, when one is following the player.
    follow: Option<FollowController>,
    /// Whether `F4` has detached the camera for a look around.
    camera_detached: bool,
    /// The camera's response to a landed hit. Fed by `CombatEvent::Hit` and by
    /// nothing else.
    shake: HitShake,
    /// The two effects, in one fixed pool.
    vfx: VfxPool,
    /// The instance staging buffer, owned here and reused every frame, so that
    /// drawing the effects allocates nothing at all.
    vfx_instances: Box<[VfxInstance; MAX_VFX_INSTANCES]>,
    last_combat_report: Instant,
}

impl Default for App {
    fn default() -> Self {
        Self {
            renderer: None,
            streaming: None,
            camera: spawn_camera(),
            controller: CameraController::default(),
            input: InputState::default(),
            last_frame: Instant::now(),
            frame_stats: FrameStats::new(),
            occluded: false,
            fatal_error: None,
            terrain: None,
            character: None,
            last_character_report: Instant::now(),
            encounter: None,
            follow: None,
            camera_detached: false,
            shake: HitShake::default(),
            vfx: VfxPool::default(),
            vfx_instances: Box::new([VfxInstance::default(); MAX_VFX_INSTANCES]),
            last_combat_report: Instant::now(),
            debug_mode: DebugMode::Off,
            debug_boxes: true,
            weather: Weather::default(),
        }
    }
}

impl App {
    fn initialize(&mut self, event_loop: &ActiveEventLoop) -> Result<(), AppRunError> {
        let attributes = Window::default_attributes()
            .with_title("Veldwake")
            .with_inner_size(LogicalSize::new(1280.0, 720.0))
            .with_visible(false);
        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .map_err(|error| AppRunError(format!("failed to create window: {error}")))?,
        );
        let initial_size = window.inner_size();
        self.camera
            .set_aspect_from_size(initial_size.width, initial_size.height);

        let profile = StreamingProfile::from_environment();
        let config = profile.config();
        let budget = UploadBudget::default();
        let world = WorldSelection::from_environment();
        let world_fingerprint = world.fingerprint();
        // The experimental disk cache is opt-in and off by default: without
        // the variable the client behaves exactly as before and touches no
        // filesystem. The client picks a directory and nothing else; the entry
        // format lives entirely inside the streaming crate.
        let cache = match std::env::var_os(CACHE_DIR_VARIABLE) {
            Some(dir) => {
                let cache_config = CacheConfig::new(PathBuf::from(dir));
                // Keyed on the selected world, so switching worlds cannot
                // replay another world's chunks: its entries read as stale.
                match ChunkCache::open(&cache_config, world_fingerprint) {
                    Ok((cache, report)) => {
                        info!(
                            entries_dir = %report.entries_dir.display(),
                            entries = report.footprint.entries,
                            disk_bytes = report.footprint.bytes,
                            swept_temporaries = report.temporaries_removed,
                            "experimental chunk cache opened"
                        );
                        Some(cache)
                    }
                    Err(error) => {
                        // A cache is an accelerator: failing to open one must
                        // never stop the client from running without it.
                        warn!(%error, "chunk cache could not be opened; continuing without it");
                        None
                    }
                }
            }
            None => None,
        };
        let streaming = StreamingBridge::with_source(
            config,
            budget,
            self.camera.position(),
            world.source(),
            cache,
        )
        .map_err(|error| AppRunError(error.to_string()))?;
        let mut renderer = pollster::block_on(Renderer::new(
            event_loop.owned_display_handle(),
            Arc::clone(&window),
            &self.camera,
        ))
        .map_err(|error| AppRunError(error.to_string()))?;

        // The character is compiled on the CPU and uploaded once: rigid
        // parts make its geometry static, so nothing here runs again.
        let selection = CharacterSelection::from_environment();
        self.terrain = world.generator();

        // The encounter comes first, because it owns both bodies when it is on
        // and the M5 preview character would otherwise draw over actor zero.
        let encounter_mode = EncounterMode::from_environment();
        if !encounter_mode.is_off() {
            let Some(generator) = self.terrain.as_ref() else {
                return Err(AppRunError(
                    "VELDWAKE_ENCOUNTER needs generated terrain; the diagnostic corridor has no \
                     walkable surface to fight on"
                        .to_owned(),
                ));
            };
            let ground = TerrainGround::new(generator);
            let scene = EncounterScene::new(encounter_mode, generator, Some(&ground))
                .map_err(|error| AppRunError(format!("the encounter did not build: {error}")))?;
            for side in SIDES {
                renderer
                    .upload_actor(
                        scene.encounter().character(side),
                        Some(scene.encounter().weapon()),
                    )
                    .map_err(|error| {
                        AppRunError(format!("{} did not upload: {error}", side.name()))
                    })?;
            }
            let stats = renderer.character_stats();
            let player = scene.encounter().combatant(Side::Player);
            let adversary = scene.encounter().combatant(Side::Adversary);
            info!(
                mode = encounter_mode.name(),
                arena = ?arena::centre().to_array(),
                arena_radius = arena::ARENA_RADIUS,
                tick_hz = veldwake_combat::COMBAT_TICK_HZ,
                max_ticks_per_frame = veldwake_combat::MAX_TICKS_PER_FRAME,
                weapon = format_args!("{:#018x}", scene.encounter().weapon().fingerprint()),
                player = format_args!("{:#018x}", scene.encounter().character(Side::Player).fingerprint()),
                adversary =
                    format_args!("{:#018x}", scene.encounter().character(Side::Adversary).fingerprint()),
                player_health = player.health().max(),
                adversary_health = adversary.health().max(),
                actors = stats.actors,
                weapons = stats.weapons,
                parts = stats.parts,
                quads = stats.quads,
                gpu_bytes = stats.vertex_bytes + stats.index_bytes + stats.uniform_bytes,
                dynamic_upload_bytes_per_frame = stats.dynamic_upload_bytes,
                world_draws = stats.world_draws,
                shadow_draws = stats.shadow_draws,
                frozen = scene.is_frozen(),
                moment_tick = scene.moment_tick(),
                played = encounter_mode.is_played(),
                clearing_level_radius = arena::LEVEL_RADIUS,
                clearing_clear_radius = arena::CLEAR_RADIUS,
                clearing_open_radius = arena::OPEN_RADIUS,
                clearing_open_rise = arena::OPEN_RISE,
                "encounter ready"
            );
            // A frozen moment with a named pose gets that pose placed against
            // the two bodies, because the fight is wherever it drifted to by the
            // tick the moment happens on. The `defeat` capture is why: aimed at
            // the arena centre, it caught the two bodies in line and showed one
            // figure standing alone. A fight being played gets a camera that
            // follows the player instead.
            let named_pose = requested_pose()
                .as_deref()
                .and_then(arena::combat_camera_pose);
            if let Some(pose) = named_pose
                && scene.is_frozen()
                && let Some((position, yaw, pitch)) = arena::frame_the_fight(
                    pose,
                    scene.encounter().combatant(Side::Player).stand_point(),
                    scene.encounter().combatant(Side::Adversary).stand_point(),
                )
            {
                self.camera.place(position, yaw, pitch);
            }
            if encounter_mode.follows_the_player() && named_pose.is_none() && !scene.is_frozen() {
                self.follow = Some(FollowController::behind(
                    scene.camera_target(),
                    player.state().facing,
                ));
            }
            self.encounter = Some(scene);
        }

        let character = if selection.is_off() || self.encounter.is_some() {
            None
        } else {
            let ground = self.terrain.as_ref().map(TerrainGround::new);
            let sampler = ground.as_ref().map(|ground| ground as &dyn GroundSampler);
            let scene = CharacterScene::new(selection, sampler).map_err(|error| {
                AppRunError(format!("requested character did not compile: {error}"))
            })?;
            renderer
                .upload_character(scene.character())
                .map_err(|error| {
                    AppRunError(format!("requested character did not upload: {error}"))
                })?;
            Some(scene)
        };
        if self.encounter.is_some() && !selection.is_off() {
            info!(
                selection = selection.name(),
                "VELDWAKE_CHARACTER is ignored while an encounter is running: the two combatants \
                 are the characters"
            );
        }
        if let Some(scene) = &character {
            let stats = renderer.character_stats();
            info!(
                selection = selection.name(),
                fingerprint = format_args!("{:#018x}", scene.character().fingerprint()),
                geometry = format_args!("{:#018x}", scene.character().geometry_fingerprint()),
                height_voxels = scene.character().body().height,
                height_units = scene.character().body().height_units(),
                parts = stats.parts,
                quads = stats.quads,
                gpu_bytes = stats.vertex_bytes + stats.index_bytes + stats.uniform_bytes,
                dynamic_upload_bytes_per_frame = stats.dynamic_upload_bytes,
                world_draws = stats.world_draws,
                shadow_draws = stats.shadow_draws,
                grounded = scene.state().grounded,
                "character ready"
            );
        } else {
            info!(selection = selection.name(), "no character in this run");
        }
        self.character = character;

        info!(
            world = world.name(),
            world_fingerprint = format_args!("{world_fingerprint:#018x}"),
            pose = requested_pose().unwrap_or_else(|| {
                resolve_pose(None).name.to_owned()
            }),
            camera_position = ?self.camera.position(),
            weather = self.weather.name(),
            profile = profile.name(),
            lod_selection = ?config.lod_selection,
            camera_chunk = ?streaming.desired_center(),
            render_radius = config.render_radius,
            dependency_halo = config.dependency_halo,
            retention_radius = config.retention_radius,
            hard_resident_cap = config.hard_resident_cap,
            max_cpu_evictions_per_update = config.max_cpu_evictions_per_update,
            max_uploads_per_frame = budget.max_uploads_per_frame,
            soft_upload_bytes_per_frame = budget.soft_bytes_per_frame,
            max_removals_per_frame = budget.max_removals_per_frame,
            "streaming bridge started"
        );
        self.last_frame = Instant::now();
        self.renderer = Some(renderer);
        self.streaming = Some(streaming);
        window.set_visible(true);
        window.request_redraw();
        info!(
            "controls: WASD move, Space/Ctrl vertical, hold right mouse to look, \
             F1 cycles debug views (off/lod/residency/boundaries/combat), F2 toggles debug boxes, F3 toggles weather, \
             Escape exits"
        );
        if self.encounter.is_some() {
            info!(
                "combat controls: WASD moves relative to the camera, J or left mouse attacks, \
                 K or Space dodges, F4 detaches the camera to look around, F1 to the combat view \
                 shows the volumes a hit is decided by"
            );
        }
        Ok(())
    }

    fn fail_and_exit(&mut self, event_loop: &ActiveEventLoop, message: impl Into<String>) {
        let message = message.into();
        tracing::error!(%message, "fatal application error");
        self.fatal_error = Some(message);
        event_loop.exit();
    }

    fn handle_keyboard(&mut self, event_loop: &ActiveEventLoop, event: winit::event::KeyEvent) {
        let pressed = event.state == ElementState::Pressed;
        let PhysicalKey::Code(code) = event.physical_key else {
            return;
        };

        if code == KeyCode::Escape && pressed {
            info!("clean shutdown requested by Escape");
            event_loop.exit();
            return;
        }

        // Combat verbs are latched on the key-down edge, because a frame can run
        // no ticks or four and a held flag would be lost or repeated.
        if let Some(action) = combat_action(code) {
            self.input.set_combat_action(action, pressed);
        }

        if pressed && let Some(action) = debug_action(code) {
            match action {
                DebugAction::CycleMode => self.debug_mode = self.debug_mode.next(),
                DebugAction::ToggleBoxes => self.debug_boxes = !self.debug_boxes,
                DebugAction::CycleWeather => self.weather = self.weather.next(),
                DebugAction::DetachCamera => {
                    self.camera_detached = !self.camera_detached;
                    // The free-fly camera starts from wherever the follow camera
                    // left off, so a detach is a step back rather than a jump.
                    info!(detached = self.camera_detached, "camera detach toggled");
                }
            }
            info!(
                mode = self.debug_mode.name(),
                boxes = self.debug_boxes,
                boxes_apply = self.debug_mode.uses_boxes(),
                weather = self.weather.name(),
                "debug view changed"
            );
            return;
        }

        if let Some(action) = camera_action(code) {
            self.input.set_action(action, pressed);
        }
    }

    fn redraw(&mut self, event_loop: &ActiveEventLoop) {
        if self.occluded {
            return;
        }

        let now = Instant::now();
        let elapsed = now.saturating_duration_since(self.last_frame);
        self.last_frame = now;
        // The camera moves before the fight does: a player's movement intent is
        // expressed in the camera's frame, so the two must not disagree by a frame.
        let following = self.follow.is_some() && !self.camera_detached;
        if following {
            let ground = self.terrain.as_ref().map(TerrainGround::new);
            let sampler = ground.as_ref().map(|ground| ground as &dyn GroundSampler);
            let target = self
                .encounter
                .as_ref()
                .map_or(self.camera.position(), EncounterScene::camera_target);
            // The impulse from last frame's hits, along this frame's view.
            let right = self.camera.planar_right();
            let offset = self.shake.offset(glam::Vec3::new(right.x, 0.0, right.y));
            if let Some(follow) = self.follow.as_mut() {
                follow.update(
                    &mut self.camera,
                    &mut self.input,
                    target,
                    offset,
                    elapsed,
                    sampler,
                );
            }
        } else {
            self.controller
                .update(&mut self.camera, &mut self.input, elapsed);
        }

        let (Some(renderer), Some(streaming)) = (self.renderer.as_mut(), self.streaming.as_mut())
        else {
            return;
        };
        // A rejected anchor keeps the previous demand center; the bridge logs
        // the transition and counts every rejected frame, so no outcome needs
        // handling here.
        streaming.track_camera(self.camera.position());
        let report = match streaming.update(renderer) {
            Ok(report) => report,
            Err(error) => {
                self.fail_and_exit(event_loop, format!("streaming runtime failed: {error}"));
                return;
            }
        };
        // The fight advances in whole ticks, however long the frame was. Its
        // geometry is never re-uploaded; a frame writes transforms only.
        let ground = self.terrain.as_ref().map(TerrainGround::new);
        let sampler = ground.as_ref().map(|ground| ground as &dyn GroundSampler);
        if let Some(scene) = self.encounter.as_mut() {
            let outcome = scene.update(elapsed, &mut self.input, &self.camera, sampler);
            for side in SIDES {
                let combatant = scene.encounter().combatant(side);
                renderer.set_actor_pose(
                    side.index(),
                    combatant.posed(),
                    Some(scene.encounter().weapon_matrix(side)),
                );
            }
            // A confirmed hit is the only thing that moves the camera. A miss, a
            // successful dodge and the start of a swing all reach here and all
            // leave it alone.
            self.shake.advance(outcome.ticks);
            self.vfx.advance(outcome.ticks);
            for event in scene.events().iter() {
                match event {
                    // One event, every response: the camera, the chips and
                    // later the sound all come from this and never from a
                    // presentation guess about what probably happened.
                    CombatEvent::Hit { point, from, .. } => {
                        self.shake.strike();
                        // The contact point the rules computed, and the
                        // direction the blow travelled: the chips leave the
                        // body the way the blade pushed it.
                        self.vfx
                            .impact(point, glam::Vec3::new(from.x, 0.35, from.y));
                    }
                    // The accent goes on the adversary's windup only. The
                    // player does not need to be told what the player just
                    // pressed.
                    CombatEvent::SwingStarted {
                        side: Side::Adversary,
                        ..
                    } => {
                        let blade = scene.encounter().blade_world(Side::Adversary);
                        self.vfx.telegraph(blade.tip);
                    }
                    CombatEvent::EncounterReset => self.vfx.clear(),
                    _ => {}
                }
            }
            // The chips first, then the readout in the space after them: one
            // buffer, one upload, one draw for both.
            let live = self.vfx.instances(self.vfx_instances.as_mut_slice());
            let right = self.camera.planar_right();
            let right = glam::Vec3::new(right.x, 0.0, right.y);
            let rows: [readout::Row; SIDES.len()] = SIDES.map(|side| {
                let combatant = scene.encounter().combatant(side);
                readout::Row {
                    centre: readout::above(
                        combatant.stand_point(),
                        scene.encounter().character(side).body().height_units(),
                    ),
                    right,
                    health: combatant.health(),
                }
            });
            let pips = readout::write(&rows, &mut self.vfx_instances[live..]);
            renderer.set_vfx_instances(&self.vfx_instances[..live + pips]);
            debug_assert!(pips <= READOUT_INSTANCES);
            self.frame_stats.record_combat(outcome, scene.events());
            if now.saturating_duration_since(self.last_combat_report) >= COMBAT_REPORT_INTERVAL {
                self.last_combat_report = now;
                let (attack_latched, dodge_latched) = self.input.combat_latches();
                report_combat(
                    scene,
                    &self.frame_stats,
                    PresentationReport {
                        shake: &self.shake,
                        vfx: &self.vfx,
                        vfx_work: renderer.vfx_frame_work(),
                    },
                    (attack_latched, dodge_latched),
                );
            }
        }
        if let Some(scene) = self.character.as_mut() {
            let posed = scene.update(elapsed.as_secs_f32(), sampler);
            renderer.set_character_pose(&posed);
            if now.saturating_duration_since(self.last_character_report)
                >= CHARACTER_REPORT_INTERVAL
            {
                self.last_character_report = now;
                let state = scene.state();
                let left = posed.contacts()[0];
                let right = posed.contacts()[1];
                info!(
                    selection = scene.selection().name(),
                    elapsed = scene.elapsed(),
                    x = state.x,
                    z = state.z,
                    base_height = state.base_height,
                    facing_degrees = state.facing.to_degrees(),
                    speed = state.speed,
                    phase = state.phase,
                    moving = posed.blend().moving,
                    run = posed.blend().run,
                    grounded = state.grounded,
                    left_stance = left.stance,
                    left_clearance = left.clearance(),
                    right_stance = right.stance,
                    right_clearance = right.clearance(),
                    landform = ground.as_ref().map(|ground| {
                        ground
                            .sample(f64::from(state.x), f64::from(state.z))
                            .landform
                            .name()
                    }),
                    zone = ground.as_ref().map(|ground| {
                        ground
                            .sample(f64::from(state.x), f64::from(state.z))
                            .zone
                            .name()
                    }),
                    "character state"
                );
            }
        }
        // Off produces no primitives, debug-slot allocations, debug uniform
        // writes, or debug draws. Fixed startup resources and any reusable
        // slots retained after prior debug use still exist.
        let primitives = match (self.debug_mode.shows_combat(), self.encounter.as_ref()) {
            (true, Some(scene)) => combat_primitives(scene.encounter(), self.debug_boxes),
            _ => debug_primitives(streaming, self.debug_mode, self.debug_boxes),
        };
        renderer.set_debug_primitives(&primitives);
        renderer.update_scene(&self.camera, self.weather, self.debug_mode.lod_tint());
        let render_started = Instant::now();
        let outcome = renderer.render();
        let renderer_render_wall_time = render_started.elapsed();
        let request_next_redraw = match outcome {
            RenderOutcome::Rendered => {
                self.frame_stats
                    .record(now, elapsed, renderer_render_wall_time, report, streaming);
                self.frame_stats.report_if_due(
                    now,
                    streaming,
                    renderer,
                    self.debug_mode,
                    self.debug_boxes,
                );
                true
            }
            RenderOutcome::Retry => true,
            RenderOutcome::Reconfigured => {
                debug!("frame skipped after surface reconfiguration");
                true
            }
            RenderOutcome::Suspended => false,
            RenderOutcome::Fatal => {
                self.fail_and_exit(event_loop, "fatal GPU or surface error");
                return;
            }
        };

        if request_next_redraw && let Some(renderer) = self.renderer.as_ref() {
            renderer.window().request_redraw();
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.renderer.is_some() {
            return;
        }

        if let Err(error) = self.initialize(event_loop) {
            self.fail_and_exit(event_loop, error.to_string());
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(renderer_window_id) = self
            .renderer
            .as_ref()
            .map(|renderer| renderer.window().id())
        else {
            return;
        };
        if window_id != renderer_window_id {
            return;
        }

        match event {
            WindowEvent::CloseRequested => {
                info!("clean shutdown requested by window");
                event_loop.exit();
            }
            WindowEvent::KeyboardInput { event, .. } => self.handle_keyboard(event_loop, event),
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Right,
                ..
            } => self.input.set_look_active(state == ElementState::Pressed),
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => self
                .input
                .set_combat_action(CombatAction::Attack, state == ElementState::Pressed),
            WindowEvent::Focused(focused) => {
                self.last_frame = Instant::now();
                if !focused {
                    self.input.reset();
                    debug!("input state reset after focus loss");
                }
            }
            WindowEvent::Resized(size) => {
                self.camera.set_aspect_from_size(size.width, size.height);
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.resize(size);
                    if size.width > 0 && size.height > 0 {
                        renderer.window().request_redraw();
                    }
                }
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(renderer) = self.renderer.as_mut() {
                    let size = renderer.window().inner_size();
                    self.camera.set_aspect_from_size(size.width, size.height);
                    renderer.resize(size);
                    if size.width > 0 && size.height > 0 {
                        renderer.window().request_redraw();
                    }
                }
            }
            WindowEvent::Occluded(occluded) => {
                self.occluded = occluded;
                self.last_frame = Instant::now();
                if !occluded && let Some(renderer) = self.renderer.as_ref() {
                    renderer.window().request_redraw();
                }
            }
            WindowEvent::RedrawRequested => self.redraw(event_loop),
            _ => {}
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        event: DeviceEvent,
    ) {
        if let DeviceEvent::MouseMotion { delta } = event {
            self.input.add_look_delta(delta.0, delta.1);
        }
    }
}

/// What the presentation layers did, for the one report line that names them.
///
/// Grouped rather than passed one by one, because a report of a fight that
/// takes nine loose arguments is a report nobody will add the tenth thing to.
struct PresentationReport<'a> {
    shake: &'a HitShake,
    vfx: &'a VfxPool,
    vfx_work: VfxFrameWork,
}

/// Two aggregate lines about the fight, on the same five-second cadence the
/// streaming and character reports use, so one capture aligns with one interval.
fn report_combat(
    scene: &EncounterScene,
    stats: &FrameStats,
    presentation: PresentationReport<'_>,
    latches: (bool, bool),
) {
    let PresentationReport {
        shake,
        vfx,
        vfx_work,
    } = presentation;
    let (attack_latched, dodge_latched) = latches;
    let input_latched = attack_latched || dodge_latched;
    let encounter = scene.encounter();
    let counters = encounter.counters();
    let player = encounter.combatant(Side::Player);
    let adversary = encounter.combatant(Side::Adversary);
    info!(
        mode = scene.mode().name(),
        armed = encounter.is_armed(),
        frozen = scene.is_frozen(),
        moment_ticks_pending = scene.pending(),
        tick = encounter.tick_index(),
        scene_ticks = scene.ticks(),
        events_this_frame = scene.events().len(),
        input_latched,
        input_attack_latched = attack_latched,
        input_dodge_latched = dodge_latched,
        distance = scene.distance(),
        player_action = player.action().label(encounter.attack_spec(Side::Player)),
        player_elapsed = player.action().elapsed(),
        player_health = player.health().current(),
        player_x = player.position().x,
        player_z = player.position().y,
        player_facing_degrees = player.state().facing.to_degrees(),
        player_grounded = player.state().grounded,
        adversary_action = adversary
            .action()
            .label(encounter.attack_spec(Side::Adversary)),
        adversary_elapsed = adversary.action().elapsed(),
        adversary_health = adversary.health().current(),
        adversary_x = adversary.position().x,
        adversary_z = adversary.position().y,
        brain = encounter.brain().state().name(),
        brain_timer = encounter.brain().timer(),
        outcome = encounter.outcome().map(Side::name),
        "combat state"
    );
    info!(
        ticks = counters.ticks,
        ticks_this_run = stats.combat_ticks,
        ticks_dropped = stats.combat_ticks_dropped,
        ticks_max_per_frame = stats.combat_ticks_max,
        frames_without_a_tick = stats.combat_frames_idle,
        clock_dropped = scene.clock().dropped(),
        clock_capped_frames = scene.clock().capped_frames(),
        player_swings = counters.swings[0],
        adversary_swings = counters.swings[1],
        player_hits = counters.hits[0],
        adversary_hits = counters.hits[1],
        player_whiffs = counters.whiffs[0],
        adversary_whiffs = counters.whiffs[1],
        player_dodges = counters.dodges[0],
        dodges_refused = counters.dodges_refused[0],
        player_staggers = counters.staggers[0],
        adversary_staggers = counters.staggers[1],
        defeats_player = counters.defeats[0],
        defeats_adversary = counters.defeats[1],
        resets = counters.resets,
        separations = counters.separations,
        blocked_moves_player = counters.blocked_moves[0],
        blocked_moves_adversary = counters.blocked_moves[1],
        hit_queries = counters.hit_queries,
        sweep_substeps_max = counters.sweep_substeps_max,
        multi_hit_suppressed = counters.multi_hit_suppressed,
        events_dropped = counters.events_dropped,
        frame_events_dropped = stats.combat_events_dropped,
        camera_strikes = shake.strikes(),
        camera_shaking = shake.is_active(),
        vfx_live = vfx.live(),
        vfx_impact_chips = vfx.live_of(VfxKind::Impact),
        vfx_telegraph_motes = vfx.live_of(VfxKind::Telegraph),
        vfx_high_water = vfx.high_water(),
        vfx_spawned = vfx.spawned(),
        vfx_dropped = vfx.dropped(),
        vfx_draws = vfx_work.draws,
        vfx_instances = vfx_work.instances,
        vfx_instance_bytes = vfx_work.bytes,
        "combat work"
    );
}

/// Keyboard control of the debug views. Separate from camera actions so the
/// mapping is testable without a window.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DebugAction {
    CycleMode,
    ToggleBoxes,
    CycleWeather,
    DetachCamera,
}

const fn debug_action(key: KeyCode) -> Option<DebugAction> {
    match key {
        KeyCode::F1 => Some(DebugAction::CycleMode),
        KeyCode::F2 => Some(DebugAction::ToggleBoxes),
        KeyCode::F3 => Some(DebugAction::CycleWeather),
        KeyCode::F4 => Some(DebugAction::DetachCamera),
        _ => None,
    }
}

/// The two combat verbs.
///
/// `Space` doubles as the dodge because the free-fly camera's vertical axis is not
/// used while a fight is on, and a keyboard alias for each verb is what lets the
/// driven smoke inject them: the evidence harness sends key events, not clicks.
const fn combat_action(key: KeyCode) -> Option<CombatAction> {
    match key {
        KeyCode::KeyJ => Some(CombatAction::Attack),
        KeyCode::KeyK | KeyCode::Space => Some(CombatAction::Dodge),
        _ => None,
    }
}

fn camera_action(key: KeyCode) -> Option<CameraAction> {
    match key {
        KeyCode::KeyW => Some(CameraAction::Forward),
        KeyCode::KeyS => Some(CameraAction::Backward),
        KeyCode::KeyA => Some(CameraAction::Left),
        KeyCode::KeyD => Some(CameraAction::Right),
        KeyCode::Space => Some(CameraAction::Up),
        KeyCode::ControlLeft | KeyCode::ControlRight => Some(CameraAction::Down),
        _ => None,
    }
}

/// Which streaming configuration the client runs. Selected by the
/// `VELDWAKE_PROFILE` environment variable so no CLI dependency is needed:
/// `default` (M3B), `m3c-baseline` (radius 3, `Lod0` only), `m3c-banded`
/// (radius 3, `Lod0`/`Lod1` band), `m4-golden` (radius 6, `Lod0` only), and
/// `m4-golden-banded` (radius 6 with the band, for the compatibility run).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StreamingProfile {
    Default,
    M3cBaseline,
    M3cBanded,
    M4Golden,
    M4GoldenBanded,
}

impl StreamingProfile {
    fn from_environment() -> Self {
        match std::env::var("VELDWAKE_PROFILE") {
            Ok(value) => match Self::parse(&value) {
                Some(profile) => profile,
                None => {
                    warn!(%value, "unknown VELDWAKE_PROFILE; using default");
                    Self::Default
                }
            },
            Err(_) => Self::Default,
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "" | "default" | "m3b" => Some(Self::Default),
            "m3c-baseline" | "baseline" => Some(Self::M3cBaseline),
            "m3c-banded" | "banded" | "lod" => Some(Self::M3cBanded),
            "m4-golden" | "m4" | "golden" => Some(Self::M4Golden),
            "m4-golden-banded" | "m4-banded" => Some(Self::M4GoldenBanded),
            _ => None,
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::M3cBaseline => "m3c-baseline",
            Self::M3cBanded => "m3c-banded",
            Self::M4Golden => "m4-golden",
            Self::M4GoldenBanded => "m4-golden-banded",
        }
    }

    const fn config(self) -> StreamingConfig {
        match self {
            Self::Default => StreamingConfig::default_profile(),
            Self::M3cBaseline => StreamingConfig::m3c_baseline(),
            Self::M3cBanded => StreamingConfig::m3c_diagnostic(),
            Self::M4Golden => StreamingConfig::m4_golden(),
            Self::M4GoldenBanded => StreamingConfig::m4_golden_banded(),
        }
    }
}

struct FrameStats {
    report_started: Instant,
    started: Instant,
    frames: u64,
    accumulated_frame_time: Duration,
    submit_total: Duration,
    submit_max: Duration,
    /// First time the runtime reported idle with every render chunk decided.
    idle_reached: Option<Duration>,
    uploads: u64,
    upload_bytes: u64,
    deactivations: u64,
    removals: u64,
    deferred_uploads: u64,
    demand_changes: u64,
    /// Combat ticks run, and the ones a frame spike had to discard.
    combat_ticks: u64,
    combat_ticks_dropped: u64,
    /// Highest number of ticks one frame ran, which is how close the catch-up cap
    /// gets to being hit.
    combat_ticks_max: u32,
    /// Frames that ran no tick at all: ordinary at a high frame rate, and the case
    /// an unlatched key press would be lost in.
    combat_frames_idle: u64,
    /// Events a frame could not hold. Must stay zero.
    combat_events_dropped: u64,
    combat_hits: [u64; 2],
    combat_swings: [u64; 2],
}

impl FrameStats {
    /// Records what one frame's combat ticks did.
    fn record_combat(
        &mut self,
        outcome: crate::encounter::FrameOutcome,
        events: &crate::encounter::FrameEvents,
    ) {
        self.combat_ticks = self.combat_ticks.saturating_add(u64::from(outcome.ticks));
        self.combat_ticks_dropped = self
            .combat_ticks_dropped
            .saturating_add(outcome.dropped_ticks);
        self.combat_ticks_max = self.combat_ticks_max.max(outcome.ticks);
        if outcome.ticks == 0 {
            self.combat_frames_idle = self.combat_frames_idle.saturating_add(1);
        }
        self.combat_events_dropped = self
            .combat_events_dropped
            .saturating_add(u64::from(events.dropped()));
        for event in events.iter() {
            match event {
                CombatEvent::Hit { attacker, .. } => {
                    let index = attacker.index();
                    self.combat_hits[index] = self.combat_hits[index].saturating_add(1);
                }
                CombatEvent::SwingStarted { side, .. } => {
                    let index = side.index();
                    self.combat_swings[index] = self.combat_swings[index].saturating_add(1);
                }
                _ => {}
            }
        }
    }

    fn new() -> Self {
        Self {
            report_started: Instant::now(),
            started: Instant::now(),
            frames: 0,
            accumulated_frame_time: Duration::ZERO,
            submit_total: Duration::ZERO,
            submit_max: Duration::ZERO,
            idle_reached: None,
            uploads: 0,
            upload_bytes: 0,
            deactivations: 0,
            removals: 0,
            deferred_uploads: 0,
            demand_changes: 0,
            combat_ticks: 0,
            combat_ticks_dropped: 0,
            combat_ticks_max: 0,
            combat_frames_idle: 0,
            combat_events_dropped: 0,
            combat_hits: [0; 2],
            combat_swings: [0; 2],
        }
    }

    fn record(
        &mut self,
        now: Instant,
        frame_time: Duration,
        renderer_render_wall_time: Duration,
        report: FrameStreamingReport,
        streaming: &StreamingBridge,
    ) {
        self.frames += 1;
        self.accumulated_frame_time += frame_time;
        self.submit_total += renderer_render_wall_time;
        self.submit_max = self.submit_max.max(renderer_render_wall_time);
        if self.idle_reached.is_none()
            && streaming.runtime().is_idle()
            && streaming.pending_removal_count() == 0
            && streaming.staged_count() == 0
            && report.uploads == 0
        {
            let elapsed = now.saturating_duration_since(self.started);
            self.idle_reached = Some(elapsed);
            let gpu_active = streaming.presented_count();
            info!(
                time_to_idle_ms = elapsed.as_secs_f64() * 1000.0,
                presented = gpu_active,
                "M3C streaming reached idle coverage"
            );
        }
        self.uploads += report.uploads as u64;
        self.upload_bytes += report.upload_bytes as u64;
        self.deactivations += report.deactivations as u64;
        self.removals += report.removals as u64;
        self.deferred_uploads += report.deferred_uploads as u64;
        self.demand_changes += u64::from(report.demand_changed);
    }

    /// One aggregate line per interval; never one line per chunk per frame.
    fn report_if_due(
        &mut self,
        now: Instant,
        streaming: &StreamingBridge,
        renderer: &Renderer,
        debug_mode: DebugMode,
        debug_boxes: bool,
    ) {
        let report_span = now.saturating_duration_since(self.report_started);
        if report_span < FRAME_REPORT_INTERVAL || self.frames == 0 {
            return;
        }

        let average_ms = self.accumulated_frame_time.as_secs_f64() * 1000.0 / self.frames as f64;
        let observed_fps = self.frames as f64 / report_span.as_secs_f64();
        let runtime = streaming.runtime();
        let demand = runtime.demand();
        let summary = runtime.summary();
        let metrics = runtime.metrics();
        let totals = streaming.totals();
        let gaps = streaming.gaps();
        let gpu = renderer.residency();
        let chunk_mesh_gpu_bytes = streaming.chunk_mesh_gpu_bytes();
        let debug_work = renderer.debug_frame_work();
        info!(
            frames = self.frames,
            report_seconds = report_span.as_secs_f64(),
            average_wall_frame_ms = average_ms,
            observed_fps,
            camera_chunk = ?runtime.center(),
            demand_changes = self.demand_changes,
            render = demand.render.len(),
            dependency = demand.dependency.len(),
            retention = demand.retention.len(),
            tracked = summary.tracked,
            cpu_resident = summary.cpu_resident,
            known_absent = summary.known_absent,
            evict_pending = summary.evict_pending,
            load_queued = summary.load_queued,
            loading = summary.loading,
            mesh_waiting = summary.mesh_waiting,
            mesh_dirty = summary.mesh_dirty,
            mesh_meshing = summary.mesh_meshing,
            mesh_ready = summary.mesh_ready,
            queued_loads = summary.queued_loads,
            queued_meshes = summary.queued_meshes,
            jobs_in_flight = summary.jobs_in_flight,
            presented = streaming.presented_count(),
            gpu_resident = gpu.resident(),
            gpu_active = gpu.active(),
            gpu_quads = gpu.quads(),
            lod0_desired = summary.lod0_desired,
            lod1_desired = summary.lod1_desired,
            lod0_ready = summary.lod0_ready,
            lod1_ready = summary.lod1_ready,
            lod0_gpu_resident = gpu.lod0.resident,
            lod0_gpu_active = gpu.lod0.active,
            lod0_gpu_quads = gpu.lod0.quads,
            lod0_gpu_bytes = gpu.lod0.bytes,
            lod1_gpu_resident = gpu.lod1.resident,
            lod1_gpu_active = gpu.lod1.active,
            lod1_gpu_quads = gpu.lod1.quads,
            lod1_gpu_bytes = gpu.lod1.bytes,
            pending_gpu_removal = streaming.pending_removal_count(),
            "M3B streaming state"
        );
        info!(
            loads_dispatched = metrics.load_jobs_dispatched,
            meshes_dispatched = metrics.mesh_jobs_dispatched,
            stale_loads = metrics.stale_load_results,
            stale_meshes = metrics.stale_mesh_results,
            fairness_loads = metrics.fairness_load_dispatches,
            hard_cap_blocks = metrics.hard_cap_blocks,
            cpu_evictions = metrics.cpu_evictions_finalized,
            eviction_budget_hits = metrics.eviction_budget_hits,
            interval_uploads = self.uploads,
            interval_upload_bytes = self.upload_bytes,
            interval_deactivations = self.deactivations,
            interval_removals = self.removals,
            interval_deferred_uploads = self.deferred_uploads,
            total_uploads = totals.uploads,
            total_upload_bytes = totals.upload_bytes,
            empty_meshes = totals.empty_meshes_presented,
            oversized_uploads = totals.oversized_uploads,
            upload_failures = totals.upload_failures,
            removal_budget_hits = totals.removal_budget_hits,
            anchor_rejections = totals.anchor_rejections,
            snapshot_bytes_dispatched = metrics.snapshot_bytes_dispatched,
            resident_payload_bytes = summary.resident_payload_bytes,
            cpu_mesh_bytes = summary.cpu_mesh_bytes,
            chunk_mesh_active_bytes = gpu.bytes(),
            total_uploads_lod0 = totals.uploads_lod0,
            total_uploads_lod1 = totals.uploads_lod1,
            total_upload_bytes_lod0 = totals.upload_bytes_lod0,
            total_upload_bytes_lod1 = totals.upload_bytes_lod1,
            lod_swaps = metrics.lod_swaps,
            stale_lod = metrics.stale_lod_results,
            gaps_closed = gaps.closed_gaps(),
            gaps_lod = gaps.lod_gaps,
            gaps_neighbor = gaps.neighbor_presentation_gaps,
            gaps_membership = gaps.membership_gaps,
            gaps_data = gaps.data_gaps,
            gaps_unattributed = gaps.unattributed_gaps,
            gap_frames_total = gaps.gap_frames_total,
            gap_frames_max = gaps.max_gap_frames,
            gap_max_simultaneous = gaps.max_simultaneous_missing,
            gap_current_missing = gaps.current_missing,
            gap_frames_with_missing = gaps.frames_with_missing,
            ready_undrawn_now = gaps.ready_undrawn_now,
            ready_undrawn_max = gaps.ready_undrawn_max,
            ready_undrawn_frames = gaps.ready_undrawn_frames,
            ready_undrawn_chunk_frames = gaps.ready_undrawn_chunk_frames,
            frontier_pipeline_pending_now = gaps.frontier_pipeline_pending_now,
            frontier_pipeline_pending_max = gaps.frontier_pipeline_pending_max,
            ready_awaiting_upload_now = gaps.ready_awaiting_upload_now,
            ready_awaiting_upload_max = gaps.ready_awaiting_upload_max,
            ready_blocked_transition_now = gaps.ready_blocked_transition_now,
            ready_blocked_transition_max = gaps.ready_blocked_transition_max,
            committed_missing_now = gaps.committed_missing_now,
            committed_missing_max = gaps.committed_missing_max,
            blocked_groups_now = gaps.blocked_groups_now,
            blocked_group_max = gaps.blocked_group_max,
            constrained_undrawn_now = gaps.constrained_undrawn_now,
            constrained_undrawn_max = gaps.constrained_undrawn_max,
            transition_commits = totals.transition_commits,
            transition_chunks = totals.transition_chunks,
            restaged = totals.restaged,
            staged_discarded = totals.staged_discarded,
            staged_now = streaming.staged_count(),
            staged_bytes = streaming.staged_bytes(),
            peak_staged_bytes = totals.peak_staged_bytes,
            committed_retained = metrics.committed_retained,
            committed_dropped = metrics.committed_dropped,
            transition_pending = summary.transition_pending,
            gpu_staged = gpu.staged(),
            gpu_staged_bytes = gpu.staged_bytes(),
            chunk_mesh_committed_bytes = chunk_mesh_gpu_bytes.committed,
            chunk_mesh_total_bytes = chunk_mesh_gpu_bytes.total(),
            peak_chunk_mesh_committed_bytes = totals.peak_chunk_mesh_committed_bytes,
            peak_chunk_mesh_total_bytes = totals.peak_chunk_mesh_total_bytes,
            presentation_commit_failures = totals.presentation_commit_failures,
            commit_invariant_failures = totals.commit_invariant_failures,
            debug_mode = debug_mode.name(),
            debug_boxes,
            debug_draws = renderer.debug_draw_count(),
            debug_slots = renderer.debug_slot_count(),
            debug_primitive_allocations = debug_work.primitive_allocations,
            debug_uniform_writes = debug_work.uniform_writes,
            snapshot_build_total_us = metrics.snapshot_build.total_us,
            snapshot_build_max_us = metrics.snapshot_build.max_us,
            lod1_derivation_total_us = metrics.lod1_derivation.total_us,
            lod1_derivation_max_us = metrics.lod1_derivation.max_us,
            worker_mesh_lod0_total_us = metrics.worker_mesh_lod0.total_us,
            worker_mesh_lod0_max_us = metrics.worker_mesh_lod0.max_us,
            worker_mesh_lod1_total_us = metrics.worker_mesh_lod1.total_us,
            worker_mesh_lod1_max_us = metrics.worker_mesh_lod1.max_us,
            cache_lookups = metrics.cache.lookups,
            cache_hits_present = metrics.cache.hits_present,
            cache_hits_absent = metrics.cache.hits_absent,
            cache_misses = metrics.cache.misses,
            cache_stale_rejects = metrics.cache.stale_rejects,
            cache_corrupt_rejects = metrics.cache.corrupt_rejects,
            cache_read_failures = metrics.cache.read_failures,
            cache_rejected_entries_removed = metrics.cache.rejected_entries_removed,
            cache_rejected_entry_delete_failures = metrics.cache.rejected_entry_delete_failures,
            cache_source_fallbacks = metrics.cache.source_fallbacks,
            cache_write_attempts = metrics.cache.write_attempts,
            cache_writes = metrics.cache.writes,
            cache_writes_skipped = metrics.cache.writes_skipped,
            cache_write_failures = metrics.cache.write_failures,
            cache_bytes_read = metrics.cache.bytes_read,
            cache_bytes_written = metrics.cache.bytes_written,
            cache_encode_total_us = metrics.cache.encode.total_us,
            cache_encode_max_us = metrics.cache.encode.max_us,
            cache_decode_total_us = metrics.cache.decode.total_us,
            cache_decode_max_us = metrics.cache.decode.max_us,
            renderer_render_wall_mean_us = self.submit_total.as_micros() / u128::from(self.frames),
            renderer_render_wall_max_us = self.submit_max.as_micros(),
            time_to_idle_ms = self.idle_reached.map(|d| d.as_secs_f64() * 1000.0),
            "M3B streaming work and budgets"
        );
        self.report_started = now;
        self.frames = 0;
        self.accumulated_frame_time = Duration::ZERO;
        self.submit_total = Duration::ZERO;
        self.submit_max = Duration::ZERO;
        self.uploads = 0;
        self.upload_bytes = 0;
        self.deactivations = 0;
        self.removals = 0;
        self.deferred_uploads = 0;
        self.demand_changes = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::{DebugAction, camera_action, combat_action, debug_action};
    use crate::debug::DebugMode;
    use crate::input::{CameraAction, CombatAction};
    use winit::keyboard::KeyCode;

    #[test]
    fn function_keys_drive_the_debug_views_and_nothing_else_does() {
        assert_eq!(debug_action(KeyCode::F1), Some(DebugAction::CycleMode));
        assert_eq!(debug_action(KeyCode::F2), Some(DebugAction::ToggleBoxes));
        assert_eq!(debug_action(KeyCode::F3), Some(DebugAction::CycleWeather));
        assert_eq!(debug_action(KeyCode::F4), Some(DebugAction::DetachCamera));
        assert_eq!(debug_action(KeyCode::F5), None);
        assert_eq!(debug_action(KeyCode::KeyW), None);
        assert_eq!(camera_action(KeyCode::F1), None);
        assert_eq!(camera_action(KeyCode::F2), None);
        assert_eq!(camera_action(KeyCode::F4), None);

        // The combat verbs have keyboard aliases so a driven smoke can inject
        // them, and they do not collide with the camera's own keys.
        assert_eq!(combat_action(KeyCode::KeyJ), Some(CombatAction::Attack));
        assert_eq!(combat_action(KeyCode::KeyK), Some(CombatAction::Dodge));
        assert_eq!(combat_action(KeyCode::Space), Some(CombatAction::Dodge));
        assert_eq!(combat_action(KeyCode::KeyW), None);
        assert_eq!(combat_action(KeyCode::F1), None);
        // `Space` is the one key with two meanings: the free-fly camera's rise and
        // the dodge. The two never apply at once, because the follow camera
        // ignores the vertical axis.
        assert_eq!(camera_action(KeyCode::Space), Some(CameraAction::Up));

        // Five presses of F1 return to the shipped rendering: M6 added the
        // combat-volume view to the cycle.
        let mut mode = DebugMode::Off;
        for _ in 0..5 {
            mode = mode.next();
        }
        assert_eq!(mode, DebugMode::Off);
    }

    #[test]
    fn streaming_profile_parses_documented_names_only() {
        use super::StreamingProfile;
        use veldwake_streaming::StreamingConfig;
        assert_eq!(
            StreamingProfile::parse("default"),
            Some(StreamingProfile::Default)
        );
        assert_eq!(StreamingProfile::parse(""), Some(StreamingProfile::Default));
        assert_eq!(
            StreamingProfile::parse(" M3C-Baseline "),
            Some(StreamingProfile::M3cBaseline)
        );
        assert_eq!(
            StreamingProfile::parse("banded"),
            Some(StreamingProfile::M3cBanded)
        );
        assert_eq!(StreamingProfile::parse("m5-character"), None);
        assert_eq!(StreamingProfile::parse("m3d"), None);
        assert_eq!(
            StreamingProfile::Default.config(),
            StreamingConfig::default()
        );
        assert_eq!(
            StreamingProfile::M3cBaseline.config(),
            StreamingConfig::m3c_baseline()
        );
        assert_eq!(
            StreamingProfile::M3cBanded.config(),
            StreamingConfig::m3c_diagnostic()
        );
    }

    #[test]
    fn platform_keys_translate_to_camera_actions() {
        assert_eq!(camera_action(KeyCode::KeyW), Some(CameraAction::Forward));
        assert_eq!(camera_action(KeyCode::KeyD), Some(CameraAction::Right));
        assert_eq!(camera_action(KeyCode::Space), Some(CameraAction::Up));
        assert_eq!(camera_action(KeyCode::KeyQ), None);
    }
}
