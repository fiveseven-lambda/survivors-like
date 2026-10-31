use std::sync::Mutex;

use bevy::ecs::entity::{Entities, EntityIndex};
use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::tasks::prelude::*;
use parry2d::bounding_volume::Aabb;
use parry2d::partitioning::{Bvh, BvhWorkspace};
use parry2d::query::PointQuery;
use rand::prelude::*;
use wasm_bindgen::prelude::*;
use wgpu::util::DeviceExt;

const VIEWPORT_RATIO: UVec2 = uvec2(16, 9);
const VIEW_SIZE: Vec2 = vec2(64.0, 36.0);
const FIELD_SIZE: UVec2 = uvec2(64 * 16, 64 * 9);
const FIELD_WIDTH: usize = FIELD_SIZE.x as usize;
const FIELD_HEIGHT: usize = FIELD_SIZE.y as usize;
const FIELD_GENERATION_STEPS: usize = 1024;
const RESOURCE_COLORS: [Color; 4] = [
    Color::hsv(160.0, 0.6, 0.8),
    Color::hsv(60.0, 0.6, 0.8),
    Color::hsv(220.0, 0.6, 0.8),
    Color::hsv(320.0, 0.6, 0.8),
];

#[wasm_bindgen]
pub fn app() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                canvas: Some(String::from("#app")),
                ..default()
            }),
            ..default()
        }))
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            update_camera.run_if(on_message::<bevy::window::WindowResized>),
        )
        .add_systems(OnEnter(State::Loading), setup_loading_screen)
        .add_systems(
            Update,
            update_loading_screen.run_if(in_state(State::Loading)),
        )
        .add_systems(OnEnter(State::Title), setup_title_screen)
        .add_systems(Update, update_title_screen.run_if(in_state(State::Title)))
        .add_systems(OnEnter(State::Running), start)
        .add_systems(
            Update,
            update_direction.run_if(in_state(State::Running).and_then(on_message::<CursorMoved>)),
        )
        .add_systems(Update, move_player.run_if(in_state(State::Running)))
        .add_systems(Update, update_player.run_if(in_state(State::Running)))
        .add_systems(
            Update,
            (move_enemies, update_enemies)
                .chain()
                .run_if(in_state(State::Running)),
        )
        .add_systems(Update, spawn_enemies.run_if(in_state(State::Running)))
        .add_systems(Update, move_bullets.run_if(in_state(State::Running)))
        .add_systems(Update, spawn_bullets.run_if(in_state(State::Running)))
        .add_systems(Update, update_counters.run_if(in_state(State::Running)))
        .add_systems(OnEnter(State::Result), setup_result_screen)
        .add_systems(Update, update_result_screen.run_if(in_state(State::Result)))
        .add_systems(OnExit(State::Result), reset)
        .insert_resource(RandomSource(StdRng::from_seed(seed())))
        .insert_state(State::Loading)
        .run();
}

#[derive(Resource)]
struct RandomSource(StdRng);

#[cfg(target_family = "wasm")]
fn seed() -> [u8; 32] {
    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(js_namespace = Math)]
        fn random() -> f64;
    }

    std::array::from_fn(|_| (random() * 256.0).floor() as u8)
}

#[cfg(not(target_family = "wasm"))]
fn seed() -> [u8; 32] {
    rand::random()
}

#[derive(Clone, PartialEq, Eq, Hash, Debug, States)]
enum State {
    Loading,
    Title,
    Running,
    Result,
}

#[derive(Component)]
struct Title;

static FIELD: Mutex<Option<Field>> = Mutex::new(None);

#[derive(Resource)]
struct Field(Vec<u8>);

#[derive(Component)]
struct PlayerTriangle(Triangle, f32);

enum Triangle {
    Inner,
    Outer,
}

#[derive(Resource)]
struct PlayerVelocity {
    direction: Vec2,
    speed: f32,
    velocity: Vec2,
}

#[derive(Resource)]
struct Player {
    position: Vec2,
    health: f32,
    max_health: f32,
    kill: f32,
    resources: [f32; 4],
}

#[derive(Component)]
struct EnemySource {
    radius: f32,
    health: u32,
    reward: f32,
    timer: std::time::Duration,
    interval: std::time::Duration,
    stroke: Handle<Mesh>,
    stroke_color: Handle<ColorMaterial>,
}

#[derive(Component)]
struct Enemy {
    radius: f32,
    health: u32,
    reward: f32,
}

#[derive(Resource)]
struct EnemiesBvh(Bvh);

#[derive(Resource)]
struct EnemiesBvhOptimizer {
    workspace: BvhWorkspace,
    counter: u32,
}

#[derive(Component)]
struct Gun {
    timer: std::time::Duration,
    interval: std::time::Duration,
    bullet: Handle<Mesh>,
    bullet_color: Handle<ColorMaterial>,
}

#[derive(Component)]
struct Bullet;

#[derive(Component)]
enum Counter {
    Kill,
    Resource(usize),
}

fn setup(
    mut commands: Commands,
    window: Single<&Window>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    commands.spawn((
        Camera2d,
        Camera {
            viewport: Some(calculate_viewport(&window)),
            ..default()
        },
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: bevy::camera::ScalingMode::Fixed {
                width: VIEW_SIZE.x,
                height: VIEW_SIZE.y,
            },
            ..OrthographicProjection::default_2d()
        }),
    ));
    commands.spawn((
        Title,
        Mesh2d(meshes.add(Rectangle::from_size(VIEW_SIZE))),
        MeshMaterial2d(materials.add(Color::hsva(0.0, 0.0, 0.0, 0.8))),
        Transform::from_xyz(0.0, 0.0, 1.0),
    ));
}

fn setup_loading_screen(mut random_source: ResMut<RandomSource>) {
    let initial_values = (0..FIELD_HEIGHT * FIELD_WIDTH)
        .map(|_| random_source.0.random())
        .collect();
    AsyncComputeTaskPool::get()
        .spawn(generate_field(initial_values))
        .detach();
}

fn update_camera(mut camera: Single<&mut Camera>, window: Single<&Window>) {
    camera.viewport = Some(calculate_viewport(&window));
}

fn calculate_viewport(window: &Window) -> bevy::camera::Viewport {
    let window_size = window.physical_size();
    let UVec2 { x, y } = window_size / VIEWPORT_RATIO;
    let size = VIEWPORT_RATIO * u32::min(x, y);
    let position = (window_size - size) / 2;
    bevy::camera::Viewport {
        physical_position: position,
        physical_size: size,
        ..default()
    }
}

async fn generate_field(initial_values: Vec<[f32; 4]>) {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::PRIMARY,
        flags: wgpu::InstanceFlags::default(),
        memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
        backend_options: wgpu::BackendOptions::default(),
        display: None,
    });
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions::default())
        .await
        .unwrap();
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor::default())
        .await
        .unwrap();
    let module = device.create_shader_module(wgpu::include_wgsl!("generate_field.wgsl"));
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &module,
        entry_point: None,
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });
    let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&initial_values),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    });
    let buffers = [
        &buffer,
        &device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: buffer.size(),
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        }),
    ];
    let bind_groups: [_; 2] = std::array::from_fn(|parity| {
        let entries: [_; 2] = std::array::from_fn(|binding| wgpu::BindGroupEntry {
            binding: binding as u32,
            resource: buffers[parity ^ binding].as_entire_binding(),
        });
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &entries,
        })
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
        pass.set_pipeline(&pipeline);
        for _ in 0..FIELD_GENERATION_STEPS {
            for bind_group in &bind_groups {
                pass.set_bind_group(0, bind_group, &[]);
                pass.dispatch_workgroups(FIELD_SIZE.x / 16, FIELD_SIZE.y / 16, 1);
            }
        }
    }
    let staging_buffer = device.create_buffer(&wgpu::wgt::BufferDescriptor {
        label: None,
        size: buffer.size(),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_buffer_to_buffer(&buffer, 0, &staging_buffer, 0, buffer.size());
    let (tx, rx) = tokio::sync::oneshot::channel();
    encoder.map_buffer_on_submit(&staging_buffer, wgpu::MapMode::Read, .., |result| {
        tx.send(result).unwrap();
    });
    queue.submit([encoder.finish()]);
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    rx.await.unwrap().unwrap();
    let values = staging_buffer.get_mapped_range(..).unwrap();
    let values: &[[f32; 4]] = bytemuck::cast_slice(&values);
    let field = values
        .iter()
        .map(|cell| {
            let max = cell.iter().copied().fold(f32::NAN, f32::max);
            cell.iter().position(|&value| value == max).unwrap_or(0) as u8
        })
        .collect();
    *FIELD.lock().unwrap() = Some(Field(field));
}

fn update_loading_screen(mut commands: Commands) {
    let field = match FIELD.try_lock() {
        Ok(ref mut message) => message.take(),
        Err(_) => return,
    };
    let Some(field) = field else {
        return;
    };
    commands.insert_resource(field);
    commands.set_state(State::Title);
}

fn setup_title_screen(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    field: Res<Field>,
) {
    let mut field_image_data = Vec::new();
    for &cell in &field.0 {
        field_image_data.extend(RESOURCE_COLORS[cell as usize].to_linear().to_u8_array());
    }
    let mut field_image = Image::new(
        Extent3d {
            height: FIELD_SIZE.y,
            width: FIELD_SIZE.x,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        field_image_data,
        TextureFormat::Rgba8Unorm,
        bevy::asset::RenderAssetUsages::RENDER_WORLD,
    );
    let mut field_image_sampler_descriptor = ImageSamplerDescriptor::nearest();
    field_image_sampler_descriptor.set_address_mode(ImageAddressMode::Repeat);
    field_image.sampler = ImageSampler::Descriptor(field_image_sampler_descriptor);
    let mut field_sprite = Sprite::from_image(images.add(field_image));
    field_sprite.rect = Some(Rect::from_center_size(Vec2::ZERO, VIEW_SIZE));
    commands.spawn((field_sprite, Transform::from_xyz(0.0, 0.0, -1.0)));

    let player_radius = 1.0;
    let player_resolution = 32;
    let player_color = materials.add(Color::WHITE);
    let theta = std::f32::consts::PI / player_resolution as f32;
    let inner_radius = 0.0;
    let (sin, cos) = f32::sin_cos(theta);
    let inner_triangle = meshes.add(Triangle2d::new(
        Vec2::new(0.0, 0.0),
        Vec2::new(-player_radius, -player_radius * sin),
        Vec2::new(-player_radius, player_radius * sin),
    ));
    for i in 0..player_resolution {
        let angle = (2 * i) as f32 * theta;
        let rotation = Quat::from_rotation_z(angle);
        let translation = player_radius * Vec2::from_angle(angle);
        commands.spawn((
            PlayerTriangle(Triangle::Inner, cos),
            Mesh2d(inner_triangle.clone()),
            MeshMaterial2d(player_color.clone()),
            Transform {
                scale: Vec3::new(1.0 - inner_radius * cos, inner_radius, 1.0),
                rotation,
                translation: translation.extend(0.0),
            },
        ));
    }
    let outer_triangle = meshes.add(Triangle2d::new(
        Vec2::new(-player_radius, 0.0),
        Vec2::new(0.0, -player_radius * sin),
        Vec2::new(0.0, player_radius * sin),
    ));
    for i in 0..player_resolution {
        let angle = (2 * i + 1) as f32 * theta;
        let rotation = Quat::from_rotation_z(angle);
        let translation = player_radius * cos * Vec2::from_angle(angle);
        commands.spawn((
            PlayerTriangle(Triangle::Outer, cos),
            Mesh2d(outer_triangle.clone()),
            MeshMaterial2d(player_color.clone()),
            Transform {
                scale: Vec3::new(cos - inner_radius, 1.0, 1.0),
                rotation,
                translation: translation.extend(0.0),
            },
        ));
    }

    commands.spawn((
        Title,
        Text2d(String::from("Click anywhere to start")),
        TextFont {
            font_size: FontSize::from(64.0),
            ..default()
        },
        Transform {
            translation: Vec2::ZERO.extend(1.0),
            rotation: Quat::IDENTITY,
            scale: Vec2::splat(1.0 / 16.0).extend(1.0),
        },
    ));

    commands.spawn((
        Counter::Kill,
        Mesh2d(meshes.add(Rectangle::new(4.0, 1.5))),
        MeshMaterial2d(materials.add(Color::BLACK)),
        Transform::from_translation(Vec3 {
            x: 2.0 - VIEW_SIZE.x / 2.0,
            y: VIEW_SIZE.y / 2.0 - 0.5,
            z: 0.0,
        }),
    ));
    commands.spawn((
        Counter::Kill,
        Text2d(String::from("0")),
        TextFont {
            font_size: FontSize::from(24.0),
            ..default()
        },
        Transform {
            translation: Vec3 {
                x: 4.0 - VIEW_SIZE.x / 2.0,
                y: VIEW_SIZE.y / 2.0 - 1.5,
                z: 0.0,
            },
            rotation: Quat::IDENTITY,
            scale: Vec2::splat(1.0 / 16.0).extend(0.0),
        },
        bevy::sprite::Anchor::BOTTOM_RIGHT,
    ));
    for (index, &color) in RESOURCE_COLORS.iter().enumerate() {
        commands.spawn((
            Counter::Resource(index),
            Mesh2d(meshes.add(Rectangle::new(4.0, 1.5))),
            MeshMaterial2d(materials.add(color)),
            Transform::from_translation(Vec3 {
                x: 2.0 - VIEW_SIZE.x / 2.0,
                y: 1.0 + VIEW_SIZE.y / 2.0 - 1.5 * (index + 2) as f32,
                z: 0.0,
            }),
        ));
        commands.spawn((
            Counter::Resource(index),
            Text2d(String::from("0")),
            TextFont {
                font_size: FontSize::from(24.0),
                ..default()
            },
            Transform {
                translation: Vec3 {
                    x: 4.0 - VIEW_SIZE.x / 2.0,
                    y: VIEW_SIZE.y / 2.0 - 1.5 * (index + 2) as f32,
                    z: 0.0,
                },
                rotation: Quat::IDENTITY,
                scale: Vec2::splat(1.0 / 16.0).extend(1.0),
            },
            bevy::sprite::Anchor::BOTTOM_RIGHT,
        ));
    }
}

fn update_title_screen(
    mut mouse_button_inputs: MessageReader<bevy::input::mouse::MouseButtonInput>,
    mut next_state: ResMut<NextState<State>>,
) {
    for input in mouse_button_inputs.read() {
        if input.button == MouseButton::Left && input.state == bevy::input::ButtonState::Released {
            next_state.set(State::Running);
        }
    }
}

fn start(
    mut commands: Commands,
    title: Query<Entity, With<Title>>,
    window: Single<&Window>,
    camera: Single<(&Camera, &GlobalTransform)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    for entity in title {
        commands.entity(entity).despawn();
    }
    let direction = calculate_direction(*window, *camera).unwrap_or(Vec2::X);
    let speed = 10.0;
    commands.insert_resource(PlayerVelocity {
        direction,
        speed,
        velocity: direction.normalize_or(Vec2::X) * speed,
    });
    commands.insert_resource(Player {
        position: Vec2::ZERO,
        health: 1.0,
        max_health: 1.0,
        kill: 0.0,
        resources: [0.0; 4],
    });
    commands.spawn(EnemySource {
        radius: 0.6,
        health: 20,
        reward: 2.0,
        timer: std::time::Duration::ZERO,
        interval: std::time::Duration::from_millis(2000),
        stroke: meshes.add(Circle::new(0.6).to_ring(0.1)),
        stroke_color: materials.add(Color::BLACK),
    });

    commands.spawn(EnemySource {
        radius: 0.4,
        health: 10,
        reward: 1.0,
        timer: std::time::Duration::ZERO,
        interval: std::time::Duration::from_millis(500),
        stroke: meshes.add(Circle::new(0.4).to_ring(0.1)),
        stroke_color: materials.add(Color::BLACK),
    });

    commands.insert_resource(EnemiesBvh(Bvh::new()));
    commands.insert_resource(EnemiesBvhOptimizer {
        workspace: BvhWorkspace::default(),
        counter: 0,
    });

    commands.spawn(Gun {
        timer: std::time::Duration::ZERO,
        interval: std::time::Duration::from_millis(400),
        bullet: meshes.add(Circle::new(0.2)),
        bullet_color: materials.add(Color::BLACK),
    });
}

fn update_direction(
    mut player: ResMut<PlayerVelocity>,
    window: Single<&Window>,
    camera: Single<(&Camera, &GlobalTransform)>,
) {
    if let Some(direction) = calculate_direction(*window, *camera) {
        player.direction = direction;
        player.velocity = direction.normalize_or(Vec2::X) * player.speed;
    }
}

fn calculate_direction(window: &Window, camera: (&Camera, &GlobalTransform)) -> Option<Vec2> {
    let cursor = window.cursor_position()?;
    camera.0.viewport_to_world_2d(camera.1, cursor).ok()
}

fn move_player(
    mut player: ResMut<Player>,
    mut background: Single<&mut Sprite>,
    player_velocity: Res<PlayerVelocity>,
    field: Res<Field>,
    time: Res<Time>,
) {
    let pos_float = player.position + player_velocity.velocity * time.delta_secs();
    let pos_int = pos_float.as_ivec2();
    let field_size = FIELD_SIZE.as_ivec2();
    let q = pos_int.div_euclid(field_size) * field_size;
    player.position = pos_float - q.as_vec2();
    let IVec2 { x: column, y: row } = pos_int - q;
    let row = (-row).rem_euclid(FIELD_HEIGHT as i32);
    let cell = field.0[row as usize * FIELD_WIDTH + column as usize];
    player.resources[cell as usize] += time.delta_secs();
    background.rect = Some(Rect::from_center_size(
        Vec2 {
            x: player.position.x,
            y: -player.position.y,
        },
        VIEW_SIZE,
    ));
}

fn update_player(
    mut next_state: ResMut<NextState<State>>,
    player: Res<Player>,
    player_triangles: Query<(&PlayerTriangle, &mut Transform)>,
) {
    let inner_radius = 1.0 - player.health / player.max_health;
    for (PlayerTriangle(triangle, cos), mut transform) in player_triangles {
        match triangle {
            Triangle::Inner => {
                transform.scale.x = 1.0 - inner_radius * cos;
                transform.scale.y = inner_radius;
            }
            Triangle::Outer => {
                transform.scale.x = cos - inner_radius;
            }
        }
    }
    if player.health <= 0.0 {
        next_state.set(State::Result);
    }
}

fn move_enemies(
    mut enemies: Query<(Entity, &Enemy, &mut Transform)>,
    enemies_bvh: Res<EnemiesBvh>,
    player: Res<PlayerVelocity>,
    time: Res<Time>,
) {
    for (entity, enemy, mut transform) in &mut enemies {
        let radius = enemy.radius;
        let old_pos = transform.translation.truncate();
        let old_aabb = Aabb::new(old_pos - Vec2::splat(radius), old_pos + Vec2::splat(radius));
        let mut enemy_velocity = -10.0 * old_pos.normalize_or_zero();
        {
            let player_distance_squared = old_pos.length_squared();
            enemy_velocity *= player_distance_squared - radius * radius;
            enemy_velocity /= player_distance_squared;
        }
        for other_index in enemies_bvh.0.intersect_aabb(&old_aabb) {
            if other_index == entity.index_u32() {
                continue;
            }
            let other_node = enemies_bvh.0.leaf_node(other_index).unwrap();
            let other_radius = (other_node.maxs().x - other_node.mins().x) / 2.0;
            let sum_radii = radius + other_radius;
            let diff = old_pos - other_node.center();
            let enemy_distance_squared = diff.length_squared();
            if enemy_distance_squared < 1000.0 * sum_radii * sum_radii {
                enemy_velocity += diff / enemy_distance_squared;
            }
        }
        let new_pos = old_pos + (enemy_velocity - player.velocity) * time.delta_secs();
        transform.translation = new_pos.extend(0.0);
    }
}

fn update_enemies(
    mut commands: Commands,
    mut enemies_bvh: ResMut<EnemiesBvh>,
    mut enemies_bvh_optimizer: ResMut<EnemiesBvhOptimizer>,
    mut player: ResMut<Player>,
    enemies: Query<(Entity, &Enemy, &Transform)>,
    time: Res<Time>,
) {
    enemies_bvh.0.refit(&mut enemies_bvh_optimizer.workspace);
    enemies_bvh_optimizer.counter += 1;
    if enemies_bvh_optimizer.counter > 10 {
        enemies_bvh
            .0
            .optimize_incremental(&mut enemies_bvh_optimizer.workspace);
        enemies_bvh_optimizer.counter = 0;
    }
    for (entity, enemy, transform) in &enemies {
        if enemy.health == 0 {
            player.kill += enemy.reward;
            commands.entity(entity).despawn();
            enemies_bvh.0.remove(entity.index_u32());
        } else {
            let size = enemy.radius;
            let new_pos = transform.translation.truncate();
            if new_pos.length() < 1.0 {
                player.health -= time.delta_secs();
            }
            let new_aabb = Aabb::new(new_pos - Vec2::splat(size), new_pos + Vec2::splat(size));
            enemies_bvh.0.insert(new_aabb, entity.index_u32());
        }
    }
}

fn spawn_enemies(
    mut commands: Commands,
    enemy_sources: Query<&mut EnemySource>,
    mut enemies_bvh: ResMut<EnemiesBvh>,
    mut random_source: ResMut<RandomSource>,
    time: Res<Time>,
) {
    for mut enemy_source in enemy_sources {
        let time = enemy_source.timer + time.delta();
        let num_enemies = (time.as_nanos() / enemy_source.interval.as_nanos()) as u32;
        enemy_source.timer = time - num_enemies * enemy_source.interval;
        let radius = enemy_source.radius;
        for _ in 0..num_enemies {
            let pos = 40.0 * Vec2::from_angle(random_source.0.random_range(0.0..360.0));
            let entity = commands.spawn((
                Enemy {
                    radius: enemy_source.radius,
                    health: enemy_source.health,
                    reward: enemy_source.reward,
                },
                Mesh2d(enemy_source.stroke.clone()),
                MeshMaterial2d(enemy_source.stroke_color.clone()),
                Transform::from_translation(pos.extend(0.0)),
            ));
            enemies_bvh.0.insert(
                Aabb::new(pos - Vec2::splat(radius), pos + Vec2::splat(radius)),
                entity.id().index_u32(),
            );
        }
    }
}

fn move_bullets(
    mut commands: Commands,
    bullets: Query<(Entity, &mut Transform), With<Bullet>>,
    entities: &Entities,
    enemies_bvh: Res<EnemiesBvh>,
    player: Res<PlayerVelocity>,
    time: Res<Time>,
) {
    for (bullet_entity, mut bullet_transform) in bullets {
        let bullet_pos = bullet_transform.translation.truncate();
        if let Some((enemy_index, (_, enemy_pos))) = enemies_bvh.0.find_best(
            f32::MAX,
            |node, _| node.aabb().distance_to_local_point(bullet_pos, true),
            |index, _| {
                let aabb = enemies_bvh.0.leaf_node(index).unwrap().aabb();
                let center = aabb.center();
                let radius = aabb.half_extents().x;
                Some((center.distance(bullet_pos) - radius, center))
            },
        ) {
            let relative_pos = enemy_pos - bullet_pos;
            if relative_pos.length() < 1.0 {
                let enemy_entity_index = EntityIndex::from_raw_u32(enemy_index).unwrap();
                let enemy_entity = entities.resolve_from_index(enemy_entity_index);
                commands.queue(move |world: &mut World| {
                    if let Ok(mut enemy_entity_mut) = world.get_entity_mut(enemy_entity) {
                        let mut enemy = enemy_entity_mut.get_mut::<Enemy>().unwrap();
                        enemy.health = enemy.health.saturating_sub(10);
                    }
                    world.despawn(bullet_entity);
                });
            } else {
                let bullet_velocity = 20.0 * relative_pos.normalize_or_zero();
                bullet_transform.translation +=
                    (time.delta_secs() * (bullet_velocity - player.velocity)).extend(0.0);
            }
        }
    }
}

fn spawn_bullets(mut commands: Commands, guns: Query<&mut Gun>, time: Res<Time>) {
    for mut gun in guns {
        let mut time = gun.timer + time.delta();
        while time >= gun.interval {
            commands.spawn((
                Bullet,
                Mesh2d(gun.bullet.clone()),
                MeshMaterial2d(gun.bullet_color.clone()),
            ));
            time -= gun.interval;
        }
        gun.timer = time;
    }
}

fn setup_result_screen(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    commands.spawn((
        Title,
        Mesh2d(meshes.add(Rectangle::from_size(VIEW_SIZE))),
        MeshMaterial2d(materials.add(Color::hsva(0.0, 0.0, 0.0, 0.8))),
        Transform::from_xyz(0.0, 0.0, 1.0),
    ));
}

fn update_result_screen(
    mut mouse_button_inputs: MessageReader<bevy::input::mouse::MouseButtonInput>,
    mut next_state: ResMut<NextState<State>>,
) {
    for input in mouse_button_inputs.read() {
        if input.button == MouseButton::Left && input.state == bevy::input::ButtonState::Released {
            next_state.set(State::Loading);
        }
    }
}

fn update_counters(resource_counter: Query<(&mut Text2d, &Counter)>, player: Res<Player>) {
    for (mut text, counter) in resource_counter {
        let value = match *counter {
            Counter::Kill => player.kill,
            Counter::Resource(index) => player.resources[index],
        };
        text.0 = value.floor().to_string();
    }
}

fn reset(mut commands: Commands) {
    commands.despawn_all::<With<Sprite>>();
    commands.despawn_all::<With<PlayerTriangle>>();
    commands.despawn_all::<With<EnemySource>>();
    commands.despawn_all::<With<Enemy>>();
    commands.despawn_all::<With<Gun>>();
    commands.despawn_all::<With<Bullet>>();
    commands.despawn_all::<With<Counter>>();
}
