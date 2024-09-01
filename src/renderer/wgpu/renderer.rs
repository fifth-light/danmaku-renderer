use log::info;
use wgpu::{
    include_wgsl, BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingType, BlendComponent, BlendFactor, BlendOperation, BlendState,
    Buffer, BufferBindingType, Color, ColorTargetState, ColorWrites, CommandEncoderDescriptor,
    Device, Extent3d, Face, FragmentState, FrontFace, IndexFormat, LoadOp, MultisampleState,
    Operations, PipelineLayoutDescriptor, PolygonMode, PrimitiveState, PrimitiveTopology, Queue,
    RenderPass, RenderPassColorAttachment, RenderPassDescriptor, RenderPipeline,
    RenderPipelineDescriptor, ShaderStages, StoreOp, SurfaceConfiguration, Texture,
    TextureDescriptor, TextureDimension, TextureFormat, TextureUsages, TextureView, VertexState,
};

use crate::{danmaku::DanmakuTime, renderer::RendererParam, worker::DanmakuParam};

use super::{
    config::ConfigUniform,
    copy::TextureCopier,
    index_buffer::IndexBuffer,
    timestamp::TimestampUniform,
    vertex_buffer::{Vertex, VertexBuffer},
    WgpuRenderCache, WgpuWorkerBuffer,
};

pub struct WgpuRenderer {
    render_pipeline: RenderPipeline,
    bind_group: BindGroup,
    timestamp_buffer: Buffer,
    config_buffer: Buffer,
    target_texture: Texture,
    target_texture_view: TextureView,
    view_formats: Vec<TextureFormat>,
    copier: TextureCopier,
}

impl WgpuRenderer {
    pub fn new(
        config: &SurfaceConfiguration,
        device: &Device,
        danmaku_param: DanmakuParam,
        renderer_param: RendererParam,
        cache: &WgpuRenderCache,
    ) -> Self {
        let vertex_shader = device.create_shader_module(include_wgsl!("vertex.wgsl"));
        let fragment_shader = device.create_shader_module(include_wgsl!("fragment.wgsl"));

        let timestamp_uniform = TimestampUniform::default();
        let timestamp_buffer = timestamp_uniform.prepare(device);

        let config_uniform: ConfigUniform = danmaku_param.clone().into();
        let config_buffer = config_uniform.prepare(device);

        let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::VERTEX,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::VERTEX,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
            label: Some("Renderer bind group layout"),
        });
        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            layout: &bind_group_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: timestamp_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: config_buffer.as_entire_binding(),
                },
            ],
            label: Some("Renderer bind group"),
        });

        let render_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("Renderer pipeline layout"),
            bind_group_layouts: &[
                &bind_group_layout,
                &cache.glyph_texture_manager.bind_group_layout,
            ],
            push_constant_ranges: &[],
        });
        let render_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: VertexState {
                module: &vertex_shader,
                entry_point: "vs_main",
                compilation_options: Default::default(),
                buffers: &[Vertex::desc()],
            },
            fragment: Some(FragmentState {
                module: &fragment_shader,
                entry_point: "fs_main",
                compilation_options: Default::default(),
                targets: &[Some(ColorTargetState {
                    format: config.format,
                    blend: Some(BlendState {
                        color: BlendComponent {
                            src_factor: BlendFactor::SrcAlpha,
                            dst_factor: BlendFactor::OneMinusSrcAlpha,
                            operation: BlendOperation::Add,
                        },
                        alpha: BlendComponent {
                            src_factor: BlendFactor::SrcAlpha,
                            dst_factor: BlendFactor::OneMinusSrcAlpha,
                            operation: BlendOperation::Add,
                        },
                    }),
                    write_mask: ColorWrites::ALL,
                })],
            }),
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: FrontFace::Ccw,
                cull_mode: Some(Face::Back),
                polygon_mode: PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        let size = Extent3d {
            width: danmaku_param.screen_size.0,
            height: danmaku_param.screen_size.1,
            depth_or_array_layers: 1,
        };
        info!("Target texture format: {:?}", config.format);
        info!("Target texture size: {:?}", size);
        let target_texture = device.create_texture(&TextureDescriptor {
            label: Some("Danmaku render target texture"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: config.format,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
            view_formats: &config.view_formats,
        });
        let target_texture_view = target_texture.create_view(&Default::default());
        let copier = TextureCopier::new(
            device,
            &target_texture,
            &target_texture_view,
            renderer_param.opacity,
        );

        Self {
            render_pipeline,
            bind_group,
            timestamp_buffer,
            config_buffer,
            target_texture,
            target_texture_view,
            view_formats: config.view_formats.clone(),
            copier,
        }
    }

    pub fn update_renderer_param(&mut self, queue: &Queue, renderer_param: RendererParam) {
        self.copier.update_opacity(queue, renderer_param.opacity);
    }

    pub fn update_danmaku_param(
        &mut self,
        device: &Device,
        queue: &Queue,
        danmaku_param: DanmakuParam,
    ) {
        let config_uniform: ConfigUniform = danmaku_param.clone().into();
        config_uniform.update(&self.config_buffer, queue);

        let size = Extent3d {
            width: danmaku_param.screen_size.0,
            height: danmaku_param.screen_size.1,
            depth_or_array_layers: 1,
        };
        info!("Target texture format: {:?}", self.target_texture.format());
        info!("Target texture size: {:?}", size);
        let target_texture = device.create_texture(&TextureDescriptor {
            label: Some("Danmaku render target texture"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: self.target_texture.format(),
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
            view_formats: &self.view_formats,
        });
        let target_texture_view = target_texture.create_view(&Default::default());
        self.copier.change_texture(device, &target_texture_view);
        self.target_texture = target_texture;
        self.target_texture_view = target_texture_view;
    }

    pub fn update(&mut self, queue: &Queue, timestamp: DanmakuTime) {
        let timestamp_uniform: TimestampUniform = timestamp.into();
        timestamp_uniform.update(&self.timestamp_buffer, queue);
    }

    fn render_vertex(
        &self,
        render_pass: &mut RenderPass,
        vertex: &VertexBuffer,
        index: &IndexBuffer,
    ) {
        let glyphs = vertex.glyphs();
        if glyphs == 0 {
            return;
        }
        render_pass.set_vertex_buffer(0, vertex.vertex_buffer.slice(..));
        render_pass.set_index_buffer(index.buffer_slice(glyphs), IndexFormat::Uint32);
        render_pass.draw_indexed(0..glyphs * 6, 0, 0..1);
    }

    pub fn render_buffer(
        &mut self,
        device: &Device,
        queue: &Queue,
        worker_buffer: &WgpuWorkerBuffer,
    ) {
        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("Danmaku render command encoder"),
        });
        let target_render_pass_desc = RenderPassDescriptor {
            label: Some("Danmaku render pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &self.target_texture_view,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::TRANSPARENT),
                    store: StoreOp::Store,
                },
            })],
            ..Default::default()
        };
        let mut target_render_pass = encoder.begin_render_pass(&target_render_pass_desc);

        target_render_pass.set_pipeline(&self.render_pipeline);
        target_render_pass.set_bind_group(0, &self.bind_group, &[]);
        target_render_pass.set_bind_group(
            1,
            &worker_buffer.cache.glyph_texture_manager.bind_group,
            &[],
        );

        if let Some(previous) = &worker_buffer.previous {
            self.render_vertex(
                &mut target_render_pass,
                previous,
                &worker_buffer.cache.index_buffer,
            );
        }
        if let Some(current) = &worker_buffer.current {
            self.render_vertex(
                &mut target_render_pass,
                current,
                &worker_buffer.cache.index_buffer,
            );
        }
        if let Some(next) = &worker_buffer.next {
            self.render_vertex(
                &mut target_render_pass,
                next,
                &worker_buffer.cache.index_buffer,
            );
        }
        drop(target_render_pass);

        queue.submit(Some(encoder.finish()));
    }

    pub fn render(&self, render_pass: &mut RenderPass) {
        self.copier.render(render_pass);
    }
}
