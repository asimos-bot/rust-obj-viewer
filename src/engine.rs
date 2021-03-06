use cgmath::InnerSpace;
use cgmath::Rotation3;
use cgmath::Zero;
use wgpu::util::DeviceExt;
use winit::window::Window;
use winit::event::DeviceEvent;

use crate::camera;
use crate::light;
use crate::model;
use crate::model::Model;
use crate::model::Mesh;
use crate::instance;
use crate::texture;

const NUM_INSTANCES_PER_ROW: u32 = 10;
const NUM_INSTANCES: u32 = NUM_INSTANCES_PER_ROW * NUM_INSTANCES_PER_ROW;
const INSTANCE_DISPLACEMENT: cgmath::Vector3<f32> = cgmath::Vector3::new(NUM_INSTANCES_PER_ROW as f32 * 0.5, 0.0, NUM_INSTANCES_PER_ROW as f32 * 0.5);

pub struct Engine {

    // create surface and adapter
    instance: wgpu::Instance,
    // open connection to GPU, creating device
    adapter: wgpu::Adapter,
    // used to interact with the GPU
    device: wgpu::Device,
    // holds the texture we will write to
    surface: wgpu::Surface,
    // used to prepare surfaces for presentation
    surface_config: wgpu::SurfaceConfiguration,
    // used to write to buffers and texture by executing recorded commands
    queue: wgpu::Queue,
    // render pipeline being used
    render_pipeline: wgpu::RenderPipeline,
    // screen size
    window_size: winit::dpi::PhysicalSize<u32>,
    // camera
    camera: camera::Camera,
    // light
    light: light::Light,
    // model
    models: Vec<model::SimpleFileModel>,
    instance_buffer: wgpu::Buffer,
    depth_texture: texture::Texture
}

impl Engine {

    pub async fn new(window: &Window) -> Self {

        let window_size = window.inner_size();
        let instance = Engine::create_instance();
        let surface = Engine::create_surface(&instance, window);
        let adapter = Engine::request_adapter(&instance, &surface).await;
        let (device, queue) = Engine::request_device_and_queue(&adapter).await;
        let surface_config = Engine::create_surface_config(&adapter, &surface, &window_size);
        surface.configure(&device, &surface_config);

        let camera_data = camera::CameraData::new((0.0, 5.0, 10.0), cgmath::Deg(-90.0), cgmath::Deg(-20.0));
        let projection = camera::Projection::new(surface_config.width, surface_config.height, cgmath::Deg(45.0), 0.1, 100.0);
        let camera_controller = camera::CameraController::new(4.0, 0.5);
        let (camera, camera_bind_group_layout) = camera::Camera::new(&device, camera_data, projection, camera_controller);

        let light_data = light::LightData::new((2.0, 2.0, 2.0), (1.0, 1.0, 1.0));
        let (light, light_bind_group_layout) = light::Light::new(&device, light_data);

        let bind_group_layouts = [&camera_bind_group_layout, &light_bind_group_layout];

        let render_pipeline = Engine::create_render_pipeline(&device, &surface_config, &bind_group_layouts);
        let models = vec![model::SimpleFileModel::new(&device, "teapot.obj").unwrap()];

        let scale = 0.05;
        let instances = (0..NUM_INSTANCES_PER_ROW).flat_map(|z| {
            (0..NUM_INSTANCES_PER_ROW).map(move |x| {
                let position = cgmath::Vector3 { x: x as f32 * 10.0, y: 0.0, z: z as f32 * 10.0 } - INSTANCE_DISPLACEMENT;

                let rotation = if position.is_zero() {
                    // this is needed so an object at (0, 0, 0) won't get scaled to zero
                    // as Quaternions can effect scale if they're not created correctly
                    cgmath::Quaternion::from_axis_angle(cgmath::Vector3::unit_z(), cgmath::Deg(0.0))
                } else {
                    cgmath::Quaternion::from_axis_angle(position.normalize(), cgmath::Deg(45.0))
                };

                instance::Instance {
                    position, rotation, scaling: cgmath::Vector3::new(scale, scale, scale)
                }
            })
        }).collect::<Vec<_>>();
        let instance_data = instances.iter().map(instance::Instance::to_raw).collect::<Vec<_>>();
        let instance_buffer = device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some("Instance Buffer"),
                contents: bytemuck::cast_slice(&instance_data),
                usage: wgpu::BufferUsages::VERTEX,
            }
        );
        let depth_texture = texture::Texture::create_depth_texture(&device, &surface_config, "depth_texture");
        Self {
            instance,
            adapter,
            device,
            surface,
            surface_config,
            queue,
            render_pipeline,
            window_size,
            camera,
            light,
            models,
            instance_buffer,
            depth_texture
        }
    }

    fn create_instance() -> wgpu::Instance {
        wgpu::Instance::new(wgpu::Backends::all())
    }
    fn create_surface(instance: &wgpu::Instance, window: &Window) -> wgpu::Surface {
        unsafe { instance.create_surface(window) }
    }
    async fn request_adapter(instance: &wgpu::Instance, surface: &wgpu::Surface) -> wgpu::Adapter {
        instance.request_adapter(
            &wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(surface),
                force_fallback_adapter: false
            }
        ).await.unwrap()
    }
    async fn request_device_and_queue(adapter: &wgpu::Adapter) -> (wgpu::Device, wgpu::Queue) {
        adapter.request_device(
            &wgpu::DeviceDescriptor {
                features: wgpu::Features::POLYGON_MODE_LINE,
                limits: wgpu::Limits::default(),
                label: Some("Engine Device")
            },
            None
        ).await.unwrap()
    }
    fn create_surface_config(adapter: &wgpu::Adapter, surface: &wgpu::Surface, window_size: &winit::dpi::PhysicalSize<u32>) -> wgpu::SurfaceConfiguration {
        wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface.get_preferred_format(&adapter).unwrap(),
            width: window_size.width,
            height: window_size.height,
            present_mode: wgpu::PresentMode::Fifo
        }
    }
    fn create_render_pipeline(device: &wgpu::Device, surface_config: &wgpu::SurfaceConfiguration, bind_group_layouts: &[&wgpu::BindGroupLayout]) -> wgpu::RenderPipeline {

        let shader = device.create_shader_module(&wgpu::ShaderModuleDescriptor {
            label: Some("Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into())
        });
        let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Render Pipeline Layout"),
            bind_group_layouts,
            push_constant_ranges: &[]
        });
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[
                    model::SimpleFileModel::describe(),
                    instance::InstanceRaw::describe()
                ]
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[wgpu::ColorTargetState {
                    format: surface_config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL
                }]
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Front),
                polygon_mode: wgpu::PolygonMode::Fill,
                clamp_depth: false,
                conservative: false
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: texture::Texture::DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default()
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false
            }
        })
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        self.camera.resize_projection(&new_size);
        if new_size.width > 0 && new_size.height > 0 {
            self.window_size = new_size;
            self.surface_config.width = new_size.width;
            self.surface_config.height = new_size.height;
            self.surface.configure(&self.device, &self.surface_config);
        }
        self.depth_texture = texture::Texture::create_depth_texture(&self.device, &self.surface_config, "depth_texture");
    }

    pub fn input(&mut self, event: &DeviceEvent) -> bool {
        self.camera.process_input(event)
    }

    pub fn update(&mut self, dt: std::time::Duration) {
        // update values
        self.camera.update_data(dt);
    }

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render Encoder")
        });
        {
            self.camera.update_buffers(&self.device, &mut encoder);
            self.light.update_buffers(&self.device, &mut encoder);
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.1,
                            g: 0.2,
                            b: 0.3,
                            a: 1.0,
                        }),
                        store: true,
                    },
                }],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_texture.view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: true
                    }),
                    stencil_ops: None
                }),
            });
            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, self.camera.get_bind_group(), &[]);
            render_pass.set_bind_group(1, self.light.get_bind_group(), &[]);

            for model in &self.models {
                render_pass.set_vertex_buffer(0, model.get_vertex_buffer().slice(..));
                render_pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
                render_pass.set_index_buffer(model.get_index_buffer().slice(..), wgpu::IndexFormat::Uint32);
                render_pass.draw_indexed(0..model.get_index_buffer_len(), 0, 0..NUM_INSTANCES as u32);
            }
        }

        // submit will accept anything that implements IntoIter
        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
    pub fn get_size(&self) -> winit::dpi::PhysicalSize<u32> {
        self.window_size.clone()
    }
}
