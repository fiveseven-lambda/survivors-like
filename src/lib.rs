use std::sync::{Arc, Mutex};

use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::tasks::prelude::*;
use rand::prelude::*;
use wasm_bindgen::prelude::*;

const VIEWPORT_RATIO: UVec2 = uvec2(16, 9);
const VIEW_SIZE: Vec2 = vec2(64.0, 36.0);
const FIELD_SIZE: UVec2 = uvec2(64 * 16, 64 * 9);
const FIELD_WIDTH: usize = FIELD_SIZE.x as usize;
const FIELD_HEIGHT: usize = FIELD_SIZE.y as usize;
const FIELD_GENERATION_STEPS: usize = 1000;
const RESOURCE_COLORS: [Color; 3] = [
    Color::hsv(120.0, 0.6, 0.8),
    Color::hsv(210.0, 0.6, 0.8),
    Color::hsv(340.0, 0.6, 0.8),
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
        .add_systems(Startup, setup_camera)
        .add_systems(
            Update,
            update_camera.run_if(on_message::<bevy::window::WindowResized>),
        )
        .add_systems(Startup, generate_field)
        .add_systems(Startup, setup_loading_screen)
        .add_systems(
            Update,
            update_loading_screen.run_if(in_state(State::Loading)),
        )
        .add_systems(OnExit(State::Loading), setup_title_screen)
        .insert_state(State::Loading)
        .run();
}

#[derive(Clone, PartialEq, Eq, Hash, Debug, States)]
enum State {
    Loading,
    Paused,
}

#[derive(Component)]
struct ProgressBar;

#[derive(Component)]
struct Loading;

#[derive(Resource)]
struct LoadingState(Arc<Mutex<Option<LoadingMessage>>>);

enum LoadingMessage {
    Loading(f32),
    Ready { field: Vec<Vec<u8>> },
}

#[derive(Resource)]
struct Field(Vec<Vec<u8>>);

fn setup_camera(mut commands: Commands, window: Single<&Window>) {
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

fn generate_field(mut commands: Commands) {
    let loading_state = Arc::new(Mutex::new(Some(LoadingMessage::Loading(0.0))));
    commands.insert_resource(LoadingState(loading_state.clone()));
    let pool = AsyncComputeTaskPool::get();
    pool.spawn(async move {
        let mut rng = StdRng::from_seed([0; 32]);
        let mut values: Vec<Vec<[f64; 3]>> = (0..FIELD_HEIGHT)
            .map(|_| (0..FIELD_WIDTH).map(|_| rng.random()).collect())
            .collect();
        for step in 0..FIELD_GENERATION_STEPS {
            values = (0..FIELD_HEIGHT)
                .map(|i| {
                    let up = if i == 0 { FIELD_HEIGHT - 1 } else { i - 1 };
                    let down = if i == FIELD_HEIGHT - 1 { 0 } else { i + 1 };
                    (0..FIELD_WIDTH)
                        .map(|j| {
                            let left = if j == 0 { FIELD_WIDTH - 1 } else { j - 1 };
                            let right = if j == FIELD_WIDTH - 1 { 0 } else { j + 1 };
                            let mut next_values = values[i][j];
                            for k in 0..3 {
                                next_values[k] += 0.4 * values[i][left][k];
                                next_values[k] += 0.4 * values[i][right][k];
                                next_values[k] += 0.4 * values[up][j][k];
                                next_values[k] += 0.4 * values[down][j][k];
                                next_values[k] += 0.1 * values[up][left][k];
                                next_values[k] += 0.1 * values[up][right][k];
                                next_values[k] += 0.1 * values[down][left][k];
                                next_values[k] += 0.1 * values[down][right][k];
                                next_values[k] /= 3.0;
                            }
                            for (k1, k2) in [(0, 1), (1, 2), (2, 0)] {
                                let reaction = 0.01
                                    * values[i][j][k1]
                                    * values[i][j][k2]
                                    * (values[i][j][k1] - values[i][j][k2]);
                                next_values[k1] += reaction;
                                next_values[k2] -= reaction;
                            }
                            next_values
                        })
                        .collect()
                })
                .collect();
            *loading_state.lock().unwrap() = Some(LoadingMessage::Loading(
                step as f32 / FIELD_GENERATION_STEPS as f32,
            ));
        }
        let field: Vec<Vec<u8>> = values
            .into_iter()
            .map(|row| {
                row.into_iter()
                    .map(|cell| {
                        let max = cell.into_iter().reduce(f64::max).unwrap();
                        cell.iter().position(|&value| value == max).unwrap_or(0) as u8
                    })
                    .collect()
            })
            .collect();
        *loading_state.lock().unwrap() = Some(LoadingMessage::Ready { field });
    })
    .detach();
}

fn setup_loading_screen(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let progress_bar = Rectangle::from_size(Vec2::new(56.0, 4.0));
    let progress_bar_border = progress_bar.to_ring(0.1);
    let white = materials.add(Color::WHITE);
    commands.spawn((
        ProgressBar,
        Loading,
        Mesh2d(meshes.add(progress_bar)),
        MeshMaterial2d(white.clone()),
        Transform {
            translation: Vec3 {
                x: -28.0,
                y: 0.0,
                z: -1.0,
            },
            rotation: Quat::IDENTITY,
            scale: Vec3 {
                x: 0.0,
                y: 1.0,
                z: 1.0,
            },
        },
    ));
    commands.spawn((
        Loading,
        Mesh2d(meshes.add(progress_bar_border)),
        MeshMaterial2d(white.clone()),
    ));
}

fn update_loading_screen(
    mut commands: Commands,
    mut progress_bar: Single<&mut Transform, With<ProgressBar>>,
    loading_state: Res<LoadingState>,
) {
    let loading_message = match loading_state.0.try_lock() {
        Ok(ref mut message) => message.take(),
        Err(_) => return,
    };
    let Some(loading_message) = loading_message else {
        return;
    };
    match loading_message {
        LoadingMessage::Loading(progress) => {
            progress_bar.translation.x = 28.0 * (progress - 1.0);
            progress_bar.scale.x = progress;
        }
        LoadingMessage::Ready { field } => {
            commands.set_state(State::Paused);
            commands.insert_resource(Field(field));
        }
    }
}

fn setup_title_screen(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    loading: Query<Entity, With<Loading>>,
    field: Res<Field>,
) {
    for entity in loading {
        commands.entity(entity).despawn();
    }
    let mut field_image_data = Vec::new();
    for row in field.0.iter().rev() {
        for &cell in row {
            field_image_data.extend(RESOURCE_COLORS[cell as usize].to_linear().to_u8_array());
        }
    }
    let mut field_image = Image::new(
        Extent3d {
            height: FIELD_HEIGHT as u32,
            width: FIELD_WIDTH as u32,
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
}
