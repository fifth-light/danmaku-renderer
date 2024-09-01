use std::{mem::size_of, num::NonZeroUsize, sync::Arc};

use bytemuck::{Pod, Zeroable};
use cosmic_text::PhysicalGlyph;
use lru::LruCache;
use wgpu::{
    util::{BufferInitDescriptor, DeviceExt},
    vertex_attr_array, Buffer, BufferAddress, BufferUsages, Device, VertexBufferLayout,
    VertexStepMode,
};

use crate::{
    danmaku::DanmakuColor,
    layout::DanmakuPosition,
    manager::{DanmakuTimeChunk, PositionedDanmakuItem},
    worker::ChunkBuffer,
};

use super::{glyph_atlas::GlyphItem, glyph_manager::GlyphTextureManager, WgpuRenderCache};

fn color_to_srgb(color: DanmakuColor) -> [f32; 3] {
    let r = (color.r() as f32 / 255.0).powf(2.2);
    let g = (color.g() as f32 / 255.0).powf(2.2);
    let b = (color.b() as f32 / 255.0).powf(2.2);
    [r, g, b]
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub(crate) struct Vertex {
    time: u32,
    track_type: u32,
    track: u32,
    line_width: u32,
    offset: [i32; 2],
    tex_coords: [u32; 2],
    color: [f32; 3],
}

impl Vertex {
    const ATTRIBS: [wgpu::VertexAttribute; 7] = vertex_attr_array![
        0 => Uint32,
        1 => Uint32,
        2 => Uint32,
        3 => Uint32,
        4 => Sint32x2,
        5 => Uint32x2,
        6 => Float32x3
    ];

    pub(crate) fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        VertexBufferLayout {
            array_stride: size_of::<Self>() as BufferAddress,
            step_mode: VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }

    fn new(
        item: &PositionedDanmakuItem,
        glyph_item: &GlyphItem,
        glyph: &PhysicalGlyph,
    ) -> [Self; 4] {
        let (track_type, track) = match item.position {
            DanmakuPosition::Scroll(track) => (0, track as u32),
            DanmakuPosition::Top(track) => (1, track as u32),
            DanmakuPosition::Bottom(track) => (2, track as u32),
        };

        let item_y = -item.item.layout_line.max_descent as i32;

        let offset_x: i32 = glyph.x + glyph_item.placement.left;
        let offset_y: i32 = glyph.y - glyph_item.placement.top + item_y;
        let width: i32 = glyph_item.placement.width.try_into().unwrap();
        let height: i32 = glyph_item.placement.height.try_into().unwrap();

        let offset_top_left = [offset_x, offset_y];
        let tex_coords_top_left = glyph_item.tex_coords;

        let offset_top_right = [offset_x + width, offset_y];
        let tex_coords_top_right = (
            glyph_item.tex_coords.0 + glyph_item.tex_size.0,
            glyph_item.tex_coords.1,
        );

        let offset_bottom_left = [offset_x, offset_y + height];
        let tex_coords_bottom_left = (
            glyph_item.tex_coords.0,
            glyph_item.tex_coords.1 + glyph_item.tex_size.1,
        );

        let offset_bottom_right = [offset_x + width, offset_y + height];
        let tex_coords_bottom_right = (
            glyph_item.tex_coords.0 + glyph_item.tex_size.0,
            glyph_item.tex_coords.1 + glyph_item.tex_size.1,
        );

        let time = item.item.time.as_millis();
        let line_width = item.item.width();
        let color = color_to_srgb(item.item.color);
        let top_left = Self {
            time,
            track_type,
            track,
            line_width,
            offset: offset_top_left,
            tex_coords: tex_coords_top_left.into(),
            color,
        };
        let top_right = Self {
            time,
            track_type,
            track,
            line_width,
            offset: offset_top_right,
            tex_coords: tex_coords_top_right.into(),
            color,
        };
        let bottom_left = Self {
            time,
            track_type,
            track,
            line_width,
            offset: offset_bottom_left,
            tex_coords: tex_coords_bottom_left.into(),
            color,
        };
        let bottom_right = Self {
            time,
            track_type,
            track,
            line_width,
            offset: offset_bottom_right,
            tex_coords: tex_coords_bottom_right.into(),
            color,
        };
        [top_left, top_right, bottom_left, bottom_right]
    }
}

pub struct VertexBuffer {
    index: u32,
    base_state_index: u32,
    glyphs: usize,
    pub(crate) vertex_buffer: Buffer,
}

impl VertexBuffer {
    fn new(
        chunk: &DanmakuTimeChunk,
        texture_manager: &GlyphTextureManager,
        device: &Device,
    ) -> Self {
        let vertexs: Vec<Vertex> = chunk
            .items
            .iter()
            .flat_map(|item| {
                item.item
                    .physical_glyphs
                    .iter()
                    .filter_map(|glyph| {
                        let glyph_item = texture_manager.find(&glyph.cache_key)?;
                        Some(Vertex::new(item, glyph_item, glyph))
                    })
                    .flatten()
            })
            .collect();
        assert_eq!(vertexs.len() % 4, 0);
        let glyphs = vertexs.len() / 4;
        let vertex_buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some(&format!(
                "Vertex for chunk #{} (based on state ${})",
                chunk.index, chunk.base_state_index
            )),
            contents: bytemuck::cast_slice(&vertexs),
            usage: BufferUsages::VERTEX,
        });
        Self {
            index: chunk.index,
            glyphs,
            base_state_index: chunk.base_state_index,
            vertex_buffer,
        }
    }

    pub fn glyphs(&self) -> u32 {
        self.glyphs.try_into().unwrap()
    }
}

impl ChunkBuffer<WgpuRenderCache> for VertexBuffer {
    fn new(chunk: &Arc<DanmakuTimeChunk>, cache: &mut WgpuRenderCache) -> Arc<Self> {
        let vertex_buffer =
            cache
                .vertex_buffer_manager
                .get(chunk, &cache.device, &mut cache.glyph_texture_manager);
        cache
            .index_buffer
            .ensure_size(&cache.device, vertex_buffer.glyphs());
        vertex_buffer
    }

    fn index(&self) -> u32 {
        self.index
    }

    fn base_state_index(&self) -> u32 {
        self.base_state_index
    }
}

pub struct VertexBufferManager {
    buffer: LruCache<(u32, u32), Arc<VertexBuffer>>,
}

impl Default for VertexBufferManager {
    fn default() -> Self {
        Self {
            buffer: LruCache::new(NonZeroUsize::new(8).unwrap()),
        }
    }
}

impl VertexBufferManager {
    pub fn clear(&mut self) {
        self.buffer.clear()
    }

    fn get(
        &mut self,
        chunk: &DanmakuTimeChunk,
        device: &Device,
        glyph_manager: &mut GlyphTextureManager,
    ) -> Arc<VertexBuffer> {
        let key = (chunk.base_state_index, chunk.index);
        self.buffer.get(&key).cloned().unwrap_or_else(|| {
            let buffer = VertexBuffer::new(chunk, glyph_manager, device);
            let buffer = Arc::new(buffer);
            self.buffer.push(key, buffer.clone());
            buffer
        })
    }
}
