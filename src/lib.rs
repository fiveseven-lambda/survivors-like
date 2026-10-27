use std::sync::Mutex;

use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::tasks::prelude::*;
use rand::prelude::*;
use wasm_bindgen::prelude::*;
use wgpu::util::DeviceExt;

const VIEWPORT_RATIO: UVec2 = uvec2(16, 9);
const VIEW_SIZE: Vec2 = vec2(64.0, 36.0);
const FIELD_SIZE: UVec2 = uvec2(64 * 16, 64 * 9);
const FIELD_WIDTH: usize = FIELD_SIZE.x as usize;
const FIELD_HEIGHT: usize = FIELD_SIZE.y as usize;
const FIELD_GENERATION_STEPS: usize = 256;
const RESOURCE_COLORS: [Color; 4] = [
    Color::hsv(60.0, 0.6, 0.8),
    Color::hsv(160.0, 0.6, 0.8),
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
        .add_systems(
            Update,
            update_loading_screen.run_if(in_state(State::Loading)),
        )
        .add_systems(OnExit(State::Loading), setup_title_screen)
        .insert_state(State::Loading)
        .run();
}

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
    Paused,
}

static FIELD: Mutex<Option<Vec<u8>>> = Mutex::new(None);

#[derive(Resource)]
struct Field(Vec<u8>);

fn setup(mut commands: Commands, window: Single<&Window>) {
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
    AsyncComputeTaskPool::get().spawn(generate_field()).detach();
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

async fn generate_field() {
    let mut rng = StdRng::from_seed(seed());
    let initial_values: Vec<[f32; 4]> = (0..FIELD_HEIGHT * FIELD_WIDTH)
        .map(|_| rng.random())
        .collect();
    let instance = wgpu::Instance::default();
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
    let field: Vec<u8> = values
        .iter()
        .map(|cell| {
            let max = cell.iter().copied().fold(f32::NAN, f32::max);
            cell.iter().position(|&value| value == max).unwrap_or(0) as u8
        })
        .collect();
    *FIELD.lock().unwrap() = Some(field);
}

fn update_loading_screen(mut commands: Commands) {
    let field = match FIELD.try_lock() {
        Ok(ref mut message) => message.take(),
        Err(_) => return,
    };
    let Some(field) = field else {
        return;
    };
    commands.set_state(State::Paused);
    commands.insert_resource(Field(field));
}

fn setup_title_screen(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
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
}
