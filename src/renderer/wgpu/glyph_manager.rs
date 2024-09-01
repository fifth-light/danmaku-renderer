use std::{
    collections::HashMap,
    mem::{self, size_of},
    num::NonZeroUsize,
};

use bytemuck::{Pod, Zeroable};
use cosmic_text::{CacheKey, FontSystem, SwashCache, SwashImage};
use log::info;
use wgpu::{
    util::{BufferInitDescriptor, DeviceExt},
    AddressMode, BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout,
    BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingResource, BindingType, Buffer,
    BufferBindingType, BufferUsages, CommandBuffer, CommandEncoderDescriptor, Device, Extent3d,
    FilterMode, Queue, Sampler, SamplerBindingType, SamplerDescriptor, ShaderStages, Texture,
    TextureDescriptor, TextureDimension, TextureFormat, TextureSampleType, TextureUsages,
    TextureView, TextureViewDescriptor, TextureViewDimension,
};

use crate::manager::DanmakuTimeChunk;

use super::{
    glyph_atlas::{GlyphItem, GlyphLayer},
    glyph_shadow::GlyphShadow,
    index_buffer::IndexBuffer,
};

#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct GlyphConfigUniform {
    pub texture_width: u32,
    pub texture_height: u32,
}

impl GlyphConfigUniform {
    pub fn prepare(&self, device: &Device) -> Buffer {
        let buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("Glyph config Buffer"),
            contents: bytemuck::cast_slice(&[*self]),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });
        buffer
    }

    pub fn update(&self, buffer: &Buffer, queue: &Queue) {
        let size: NonZeroUsize = size_of::<GlyphConfigUniform>().try_into().unwrap();
        let mut buffer = queue
            .write_buffer_with(buffer, 0, size.try_into().unwrap())
            .unwrap();
        buffer.copy_from_slice(bytemuck::cast_slice(&[*self]));
    }
}

pub struct GlyphTextureManager {
    texture_size: (u32, u32),
    swash_cache: SwashCache,
    pub(crate) texture: Texture,
    pub(crate) shadow_texture: Texture,
    sampler: Sampler,
    pub(crate) bind_group_layout: BindGroupLayout,
    pub(crate) bind_group: BindGroup,
    layer: GlyphLayer,
    glyphs: HashMap<CacheKey, Option<GlyphItem>>,
    config_uniform: GlyphConfigUniform,
    config_buffer: Buffer,
    shadow_width: u32,
    shadow: GlyphShadow,
}

impl GlyphTextureManager {
    pub fn new(
        texture_size: (u32, u32),
        device: &Device,
        shadow_width: u32,
        shadow_weight: f32,
    ) -> Self {
        let config_uniform = GlyphConfigUniform {
            texture_width: texture_size.0,
            texture_height: texture_size.1,
        };
        let config_buffer = config_uniform.prepare(device);
        let sampler = device.create_sampler(&SamplerDescriptor {
            address_mode_u: AddressMode::ClampToEdge,
            address_mode_v: AddressMode::ClampToEdge,
            address_mode_w: AddressMode::ClampToEdge,
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Nearest,
            mipmap_filter: FilterMode::Nearest,
            ..Default::default()
        });
        let size = Extent3d {
            width: texture_size.0,
            height: texture_size.1,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&TextureDescriptor {
            label: Some("Glyph texture"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::R8Unorm,
            usage: TextureUsages::TEXTURE_BINDING
                | TextureUsages::COPY_DST
                | TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let shadow_texture = device.create_texture(&TextureDescriptor {
            label: Some("Shadow texture"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::R8Unorm,
            usage: TextureUsages::TEXTURE_BINDING
                | TextureUsages::RENDER_ATTACHMENT
                | TextureUsages::COPY_DST
                | TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let texture_view = texture.create_view(&TextureViewDescriptor {
            dimension: Some(TextureViewDimension::D2),
            ..Default::default()
        });
        let shadow_texture_view = shadow_texture.create_view(&TextureViewDescriptor {
            dimension: Some(TextureViewDimension::D2),
            ..Default::default()
        });
        let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("Glyph texture bind group layout"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 3,
                    visibility: ShaderStages::FRAGMENT,
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
            label: Some("Glyph texture bind group"),
            layout: &bind_group_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(&texture_view),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::TextureView(&shadow_texture_view),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: BindingResource::Sampler(&sampler),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: config_buffer.as_entire_binding(),
                },
            ],
        });

        let layer = GlyphLayer::new(texture_size);
        let shadow = GlyphShadow::new(
            device,
            &texture,
            &texture_view,
            texture_size,
            shadow_width,
            shadow_weight,
        );

        Self {
            texture_size,
            swash_cache: SwashCache::new(),
            texture,
            shadow_texture,
            bind_group_layout,
            sampler,
            bind_group,
            layer,
            glyphs: Default::default(),
            config_uniform,
            config_buffer,
            shadow_width,
            shadow,
        }
    }

    // TODO: Copy the texture to memory if out of display memory
    fn copy_texture(
        &mut self,
        device: &Device,
        new_size: Extent3d,
    ) -> (CommandBuffer, TextureView) {
        let old_size = Extent3d {
            width: self.texture_size.0,
            height: self.texture_size.1,
            depth_or_array_layers: 1,
        };

        let new_texture = device.create_texture(&TextureDescriptor {
            label: Some("Glyph texture"),
            size: new_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::R8Unorm,
            usage: TextureUsages::TEXTURE_BINDING
                | TextureUsages::COPY_DST
                | TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let new_shadow_texture = device.create_texture(&TextureDescriptor {
            label: Some("Shadow texture"),
            size: new_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::R8Unorm,
            usage: TextureUsages::TEXTURE_BINDING
                | TextureUsages::RENDER_ATTACHMENT
                | TextureUsages::COPY_DST
                | TextureUsages::COPY_SRC,
            view_formats: &[],
        });

        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("Texture growing commands"),
        });
        encoder.copy_texture_to_texture(
            self.texture.as_image_copy(),
            new_texture.as_image_copy(),
            old_size,
        );
        encoder.copy_texture_to_texture(
            self.shadow_texture.as_image_copy(),
            new_shadow_texture.as_image_copy(),
            old_size,
        );

        let new_texture_view = new_texture.create_view(&TextureViewDescriptor {
            dimension: Some(TextureViewDimension::D2),
            ..Default::default()
        });
        let new_shadow_texture_view = new_shadow_texture.create_view(&TextureViewDescriptor {
            dimension: Some(TextureViewDimension::D2),
            ..Default::default()
        });
        let new_bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("Glyph texture bind group layout"),
            layout: &self.bind_group_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(&new_texture_view),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::TextureView(&new_shadow_texture_view),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: BindingResource::Sampler(&self.sampler),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: self.config_buffer.as_entire_binding(),
                },
            ],
        });

        self.texture = new_texture;
        self.shadow_texture = new_shadow_texture;
        self.bind_group = new_bind_group;

        (encoder.finish(), new_texture_view)
    }

    #[must_use]
    fn grow_texture(&mut self, device: &Device, queue: &Queue) -> CommandBuffer {
        let new_size = (self.texture_size.0 * 2, self.texture_size.1 * 2);
        let new_texture_size = Extent3d {
            width: new_size.0,
            height: new_size.1,
            depth_or_array_layers: 1,
        };
        info!("Grow texture to {}x{}", new_size.0, new_size.1);

        self.config_uniform.texture_width = new_size.0;
        self.config_uniform.texture_height = new_size.1;
        self.config_uniform.update(&self.config_buffer, queue);

        let (buffer, texture_view) = self.copy_texture(device, new_texture_size);
        self.texture_size = new_size;
        self.layer.grow(new_size);
        self.shadow
            .update_texture(device, queue, new_size, &texture_view);

        buffer
    }

    pub fn find(&self, glyph: &CacheKey) -> Option<&GlyphItem> {
        self.glyphs.get(glyph).and_then(|item| item.as_ref())
    }

    fn exists(&self, glyph: &CacheKey) -> bool {
        self.glyphs.contains_key(glyph)
    }

    fn insert_glyph(
        &mut self,
        device: &Device,
        queue: &Queue,
        glyph: &CacheKey,
        image: &SwashImage,
        command_buffer: &mut Vec<CommandBuffer>,
    ) {
        if image.placement.width == 0 || image.placement.height == 0 {
            self.glyphs.insert(*glyph, None);
            return;
        }
        if let Some(item) = self
            .layer
            .new_item(&self.texture, queue, image, self.shadow_width)
        {
            self.shadow.new_glyph(&item);
            self.glyphs.insert(*glyph, Some(item));
        } else {
            command_buffer.push(self.grow_texture(device, queue));
            let pending_buffer = mem::take(command_buffer);
            queue.submit(pending_buffer);
            command_buffer.clear();

            let item = self
                .layer
                .new_item(&self.texture, queue, image, self.shadow_width)
                .expect("Glyph too large");
            self.shadow.new_glyph(&item);
            self.glyphs.insert(*glyph, Some(item));
        }
    }

    pub fn generate(
        &mut self,
        device: &Device,
        queue: &Queue,
        font_system: &mut FontSystem,
        chunk: &DanmakuTimeChunk,
        command_buffer: &mut Vec<CommandBuffer>,
    ) {
        for glyph in chunk.glyph_ids() {
            if self.exists(glyph) {
                continue;
            }
            let image = self.swash_cache.get_image_uncached(font_system, *glyph);
            let image = match image {
                Some(image) => image,
                None => continue,
            };
            self.insert_glyph(device, queue, glyph, &image, command_buffer);
        }
    }

    pub fn flush(
        &mut self,
        device: &Device,
        index_buffer: &mut IndexBuffer,
    ) -> Option<CommandBuffer> {
        self.shadow.draw(device, &self.shadow_texture, index_buffer)
    }

    pub fn clear(&mut self) {
        self.glyphs.clear();
        self.shadow.clear();
        self.layer.clear();
    }

    pub fn new_param(&mut self, queue: &Queue, shadow_width: u32, shadow_weight: f32) {
        self.shadow.new_param(queue, shadow_width, shadow_weight);
    }
}
