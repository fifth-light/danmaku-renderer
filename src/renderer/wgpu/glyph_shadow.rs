use std::{mem::size_of, num::NonZeroUsize};

use bytemuck::{Pod, Zeroable};
use wgpu::{
    util::{BufferInitDescriptor, DeviceExt},
    vertex_attr_array, AddressMode, BindGroup, BindGroupDescriptor, BindGroupEntry,
    BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingResource, BindingType,
    BlendState, Buffer, BufferAddress, BufferBindingType, BufferUsages, ColorTargetState,
    ColorWrites, CommandBuffer, Device, Face, FilterMode, FragmentState, FrontFace, IndexFormat,
    MultisampleState, PipelineLayoutDescriptor, PolygonMode, PrimitiveState, PrimitiveTopology,
    Queue, RenderPipeline, RenderPipelineDescriptor, Sampler, SamplerBindingType,
    SamplerDescriptor, ShaderModuleDescriptor, ShaderSource, ShaderStages, Texture,
    TextureSampleType, TextureView, TextureViewDescriptor, TextureViewDimension,
    VertexBufferLayout, VertexState, VertexStepMode,
};

use super::{glyph_atlas::GlyphItem, index_buffer::IndexBuffer};

#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct ShadowConfigUniform {
    pub shadow_width: u32,
    pub shadow_weight: f32,
    pub texture_width: u32,
    pub texture_height: u32,
    pub pixel_width: f32,
    pub pixel_height: f32,
}

impl ShadowConfigUniform {
    pub fn prepare(&self, device: &Device) -> Buffer {
        let buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("Shadow config Buffer"),
            contents: bytemuck::cast_slice(&[*self]),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });
        buffer
    }

    pub fn update(&self, buffer: &Buffer, queue: &Queue) {
        let size: NonZeroUsize = size_of::<ShadowConfigUniform>().try_into().unwrap();
        let mut buffer = queue
            .write_buffer_with(buffer, 0, size.try_into().unwrap())
            .unwrap();
        buffer.copy_from_slice(bytemuck::cast_slice(&[*self]));
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [u32; 2],
}

impl Vertex {
    const ATTRIBS: [wgpu::VertexAttribute; 1] = vertex_attr_array![
        0 => Uint32x2
    ];

    fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        VertexBufferLayout {
            array_stride: size_of::<Self>() as BufferAddress,
            step_mode: VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }

    fn new(tex_coords: (u32, u32), tex_size: (u32, u32)) -> [Vertex; 4] {
        let top_left = Self {
            position: [tex_coords.0, tex_coords.1],
        };
        let top_right = Self {
            position: [tex_coords.0 + tex_size.0, tex_coords.1],
        };
        let bottom_left = Self {
            position: [tex_coords.0, tex_coords.1 + tex_size.1],
        };
        let bottom_right = Self {
            position: [tex_coords.0 + tex_size.0, tex_coords.1 + tex_size.1],
        };
        [top_left, top_right, bottom_left, bottom_right]
    }
}

pub(crate) struct GlyphShadow {
    render_pipeline: RenderPipeline,
    sampler: Sampler,
    bind_group_layout: BindGroupLayout,
    bind_group: BindGroup,
    config_uniform: ShadowConfigUniform,
    config_buffer: Buffer,
    glyphs: u32,
    vertexs: Vec<Vertex>,
}

impl GlyphShadow {
    pub fn new(
        device: &Device,
        texture: &Texture,
        texture_view: &TextureView,
        texture_size: (u32, u32),
        shadow_width: u32,
        shadow_weight: f32,
    ) -> Self {
        let config_uniform = ShadowConfigUniform {
            shadow_width,
            shadow_weight,
            texture_width: texture_size.0,
            texture_height: texture_size.1,
            pixel_width: 1.0 / texture_size.0 as f32,
            pixel_height: 1.0 / texture_size.1 as f32,
        };
        let config_buffer = config_uniform.prepare(device);

        let sampler = device.create_sampler(&SamplerDescriptor {
            address_mode_u: AddressMode::ClampToEdge,
            address_mode_v: AddressMode::ClampToEdge,
            address_mode_w: AddressMode::ClampToEdge,
            mag_filter: FilterMode::Nearest,
            min_filter: FilterMode::Nearest,
            mipmap_filter: FilterMode::Nearest,
            ..Default::default()
        });

        let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("Shadow texture bind layout"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        multisampled: false,
                        view_dimension: TextureViewDimension::D2,
                        sample_type: TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::FRAGMENT | ShaderStages::VERTEX,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("Shadow bind group"),
            layout: &bind_group_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(texture_view),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::Sampler(&sampler),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: config_buffer.as_entire_binding(),
                },
            ],
        });

        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("Shadow shader"),
            source: ShaderSource::Wgsl(include_str!("shadow.wgsl").into()),
        });

        let render_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("Shadow pipeline layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let render_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("Shadow render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: "vs_main",
                compilation_options: Default::default(),
                buffers: &[Vertex::desc()],
            },
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: "fs_main",
                compilation_options: Default::default(),
                targets: &[Some(ColorTargetState {
                    format: texture.format(),
                    blend: Some(BlendState::REPLACE),
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

        Self {
            render_pipeline,
            sampler,
            bind_group,
            bind_group_layout,
            config_uniform,
            config_buffer,
            glyphs: 0,
            vertexs: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        self.glyphs = 0;
        self.vertexs.clear();
    }

    pub fn new_glyph(&mut self, item: &GlyphItem) {
        self.glyphs += 1;
        self.vertexs.reserve(4);
        Vertex::new(item.tex_coords, item.tex_size)
            .into_iter()
            .for_each(|item| self.vertexs.push(item))
    }

    pub fn draw(
        &mut self,
        device: &Device,
        shadow_texture: &Texture,
        index_buffer: &mut IndexBuffer,
    ) -> Option<CommandBuffer> {
        if self.glyphs == 0 {
            return None;
        }
        index_buffer.ensure_size(device, self.glyphs);

        let shadow_texture_view = shadow_texture.create_view(&TextureViewDescriptor {
            dimension: Some(TextureViewDimension::D2),
            ..Default::default()
        });

        let vertex_buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("Vertex for shadow"),
            contents: bytemuck::cast_slice(&self.vertexs),
            usage: BufferUsages::VERTEX,
        });

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Shadow render encoder"),
        });
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Shadow render pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &shadow_texture_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });

        render_pass.set_pipeline(&self.render_pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        render_pass.set_index_buffer(index_buffer.buffer_slice(self.glyphs), IndexFormat::Uint32);
        render_pass.draw_indexed(0..self.glyphs * 6, 0, 0..1);

        drop(render_pass);
        self.clear();

        Some(encoder.finish())
    }

    pub fn update_texture(
        &mut self,
        device: &Device,
        queue: &Queue,
        texture_size: (u32, u32),
        texture_view: &TextureView,
    ) {
        self.config_uniform.texture_width = texture_size.0;
        self.config_uniform.texture_height = texture_size.1;
        self.config_uniform.pixel_width = 1.0 / texture_size.0 as f32;
        self.config_uniform.pixel_height = 1.0 / texture_size.1 as f32;
        self.config_uniform.update(&self.config_buffer, queue);

        self.bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("Shadow bind group"),
            layout: &self.bind_group_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(texture_view),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::Sampler(&self.sampler),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: self.config_buffer.as_entire_binding(),
                },
            ],
        });
    }

    pub fn new_param(&mut self, queue: &Queue, shadow_width: u32, shadow_weight: f32) {
        self.config_uniform.shadow_width = shadow_width;
        self.config_uniform.shadow_weight = shadow_weight;
        self.config_uniform.update(&self.config_buffer, queue);
    }
}
