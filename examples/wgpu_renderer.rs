use std::{
    iter,
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use cosmic_text::{Attrs, AttrsList, Family, FontSystem, ShapeBuffer, Weight};
use danmaku_renderer::{
    danmaku::DanmakuTime,
    layout::LayoutMode,
    renderer::{
        wgpu::{WgpuRenderCache, WgpuRenderer, WgpuWorkerBuffer, WgpuWorkerManager},
        RendererParam,
    },
    sources::bilibili::parse_xml_from_file,
    worker::{DanmakuParam, WorkerBuffer, WorkerManager, WorkerState},
};
use fps_counter::FPSCounter;
use futures::executor::block_on;
use log::info;
use wgpu::{
    util::{backend_bits_from_env, initialize_adapter_from_env, power_preference_from_env},
    Backends, Device, DeviceDescriptor, Instance, InstanceDescriptor, PowerPreference, PresentMode,
    Queue, RenderPass, RequestAdapterOptions, Surface, SurfaceConfiguration, SurfaceError,
    TextureFormat, TextureUsages,
};
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalSize},
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowAttributes, WindowId},
};

fn create_param(screen_size: PhysicalSize<u32>) -> DanmakuParam {
    let attrs = Attrs::new();
    attrs.family(Family::SansSerif);
    attrs.weight(Weight::BOLD);
    DanmakuParam {
        screen_size: (screen_size.width, screen_size.height),
        lifetime: Duration::from_secs(8),
        font_size: 28.0,
        line_height: 32,
        font_attrs: AttrsList::new(attrs),
        layout_mode: LayoutMode::ShowAll,
        shadow_size: 3,
        shadow_weight: 1.5,
    }
}

struct AppSurface<'a> {
    surface: Surface<'a>,
    device: Arc<Device>,
    queue: Arc<Queue>,
    config: SurfaceConfiguration,
}

impl<'a> AppSurface<'a> {
    pub async fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();
        let instance = Instance::new(InstanceDescriptor {
            backends: backend_bits_from_env().unwrap_or(Backends::all()),
            ..Default::default()
        });
        let surface = instance.create_surface(window.clone()).unwrap();
        let adapter = initialize_adapter_from_env(&instance, Some(&surface));
        let adapter = match adapter {
            Some(adapter) => adapter,
            None => instance
                .request_adapter(&RequestAdapterOptions {
                    power_preference: power_preference_from_env().unwrap_or(PowerPreference::None),
                    ..Default::default()
                })
                .await
                .expect("Unable to initialize graphics adapter"),
        };
        info!("Adapter backend: {}", adapter.get_info().backend);
        let (device, queue) = adapter
            .request_device(&DeviceDescriptor::default(), None)
            .await
            .unwrap();
        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .or_else(|| surface_caps.formats.first().copied())
            .unwrap_or(TextureFormat::Bgra8UnormSrgb);
        let config = SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: PresentMode::AutoVsync,
            alpha_mode: surface_caps.alpha_modes[0],
            desired_maximum_frame_latency: 2,
            view_formats: vec![],
        };
        surface.configure(&device, &config);
        Self {
            surface,
            device: Arc::new(device),
            queue: Arc::new(queue),
            config,
        }
    }

    fn resize(&mut self, new_size: PhysicalSize<u32>) {
        self.config.width = new_size.width;
        self.config.height = new_size.height;
        self.surface.configure(&self.device, &self.config);
    }
}

struct DanmakuRenderer {
    renderer: WgpuRenderer,
    buffer: Arc<Mutex<WgpuWorkerBuffer>>,
    worker: WgpuWorkerManager,
    start_time: Instant,
    param: DanmakuParam,
}

impl DanmakuRenderer {
    fn new(surface: &AppSurface, param: DanmakuParam) -> Self {
        let path = Path::new("test/1176840_history.xml");
        let source = parse_xml_from_file(path).unwrap();
        let font_system = FontSystem::new();
        let shape_buffer = ShapeBuffer::default();
        let cache = WgpuRenderCache::new(
            surface.device.clone(),
            surface.queue.clone(),
            (256, 256),
            param.clone(),
        );
        let renderer = WgpuRenderer::new(
            &surface.config,
            &surface.device,
            param.clone(),
            RendererParam { opacity: 1.0 },
            &cache,
        );
        let buffer = WorkerBuffer::new(cache);

        let buffer = Arc::new(Mutex::new(buffer));
        let state = WorkerState {
            buffer: buffer.clone(),
            font_system,
            shape_buffer,
            source: Box::new(source),
        };
        let mut worker = WorkerManager::new(param.clone(), state);
        worker.request(None, 0).unwrap();

        Self {
            renderer,
            buffer,
            worker,
            start_time: Instant::now(),
            param,
        }
    }

    fn update_param(&mut self, surface: &AppSurface, new_param: DanmakuParam) {
        self.worker.change_param(new_param.clone()).unwrap();
        self.renderer
            .update_danmaku_param(&surface.device, &surface.queue, new_param.clone());
        self.param = new_param;
    }

    fn render_buffer(&mut self, surface: &AppSurface) {
        let timestamp = self.start_time.elapsed();
        let timestamp = DanmakuTime::from_millis(timestamp.as_millis() as u32);
        self.renderer.update(&surface.queue, timestamp);

        let buffer = self.buffer.lock().unwrap();
        let lifetime = self.param.lifetime.as_millis() as u32;
        let index = timestamp.as_millis() / lifetime;
        if buffer.should_request_worker(index) {
            self.worker.request(None, index).unwrap();
        }
        self.renderer
            .render_buffer(&surface.device, &surface.queue, &buffer);
    }

    fn render(&mut self, render_pass: &mut RenderPass) {
        self.renderer.render(render_pass)
    }
}

struct State<'a> {
    surface: AppSurface<'a>,
    size: PhysicalSize<u32>,
    fps_counter: FPSCounter,
    danmaku_renderer: DanmakuRenderer,

    window: Arc<Window>,
}

impl<'a> State<'a> {
    async fn new(window: Arc<Window>) -> Self {
        let fps_counter = FPSCounter::new();

        let size = window.inner_size();
        let param = create_param(size);
        let surface = AppSurface::new(window.clone()).await;

        let danmaku_renderer = DanmakuRenderer::new(&surface, param);

        State {
            surface,
            size,
            fps_counter,
            danmaku_renderer,
            window,
        }
    }

    fn render(&mut self) -> Result<(), SurfaceError> {
        let fps = self.fps_counter.tick();
        self.window.set_title(&format!("FPS: {}", fps));

        self.danmaku_renderer.render_buffer(&self.surface);

        let output = self.surface.surface.get_current_texture()?;
        let texture_view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder =
            self.surface
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Render Encoder"),
                });
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &texture_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.1,
                        g: 0.1,
                        b: 0.1,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });
        self.danmaku_renderer.render(&mut render_pass);
        drop(render_pass);
        self.surface.queue.submit(iter::once(encoder.finish()));

        self.window.pre_present_notify();

        output.present();

        self.window.request_redraw();
        Ok(())
    }

    fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            let new_param = create_param(new_size);

            self.size = new_size;
            self.surface.resize(new_size);
            self.danmaku_renderer.update_param(&self.surface, new_param);
        }
    }
}

#[derive(Default)]
struct App {
    window: Option<Arc<Window>>,
    state: Option<State<'static>>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let attributes =
                WindowAttributes::default().with_inner_size(LogicalSize::new(2048, 2048));
            let window = event_loop.create_window(attributes).unwrap();
            self.window = Some(Arc::new(window));
        }
        let window = self.window.as_ref().unwrap();
        if self.state.is_none() {
            self.state = Some(block_on(State::new(window.clone())));
        }
    }

    fn suspended(&mut self, _: &ActiveEventLoop) {
        self.state = None;
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                if let Some(state) = self.state.as_mut() {
                    match state.render() {
                        Ok(_) => {}
                        Err(wgpu::SurfaceError::Lost) => state.resize(state.size),
                        Err(wgpu::SurfaceError::OutOfMemory) => event_loop.exit(),
                        Err(e) => panic!("Render failed: {:?}", e),
                    }
                }
            }
            WindowEvent::Resized(physical_size) => {
                if let Some(state) = self.state.as_mut() {
                    state.resize(physical_size);
                }
            }
            _ => (),
        }
    }
}

fn main() {
    env_logger::init();
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Wait);

    let mut app = App::default();
    event_loop.run_app(&mut app).unwrap();
}
