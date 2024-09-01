use std::{mem, sync::Arc};

use cosmic_text::FontSystem;
use wgpu::{CommandBuffer, Device, Queue};

use crate::{
    manager::DanmakuTimeChunk,
    worker::{DanmakuParam, RenderCache},
};

use super::{
    glyph_manager::GlyphTextureManager, index_buffer::IndexBuffer,
    vertex_buffer::VertexBufferManager,
};

pub struct WgpuRenderCache {
    pub(crate) device: Arc<Device>,
    pub(crate) queue: Arc<Queue>,
    pub(crate) glyph_texture_manager: GlyphTextureManager,
    pub(crate) vertex_buffer_manager: VertexBufferManager,
    pub(crate) index_buffer: IndexBuffer,
    command_buffers: Vec<CommandBuffer>,
    danmaku_param: DanmakuParam,
}

impl WgpuRenderCache {
    pub fn new(
        device: Arc<Device>,
        queue: Arc<Queue>,
        texture_size: (u32, u32),
        danmaku_param: DanmakuParam,
    ) -> Self {
        let glyph_texture_manager = GlyphTextureManager::new(
            texture_size,
            &device,
            danmaku_param.shadow_size,
            danmaku_param.shadow_weight,
        );
        WgpuRenderCache {
            device,
            queue,
            glyph_texture_manager,
            vertex_buffer_manager: Default::default(),
            index_buffer: Default::default(),
            danmaku_param,
            command_buffers: Vec::new(),
        }
    }
}

impl RenderCache for WgpuRenderCache {
    fn new_param(&mut self, new_param: DanmakuParam) {
        self.vertex_buffer_manager.clear();
        if (new_param.font_size != self.danmaku_param.font_size)
            || (new_param.font_attrs != self.danmaku_param.font_attrs)
            || (new_param.shadow_size != self.danmaku_param.shadow_size)
            || (new_param.shadow_weight != self.danmaku_param.shadow_weight)
        {
            self.glyph_texture_manager.clear();
            self.glyph_texture_manager.new_param(
                &self.queue,
                new_param.shadow_size,
                new_param.shadow_weight,
            );
        }
        self.danmaku_param = new_param;
    }

    fn prepare(&mut self, font_system: &mut FontSystem, chunk: &DanmakuTimeChunk) {
        self.glyph_texture_manager.generate(
            &self.device,
            &self.queue,
            font_system,
            chunk,
            &mut self.command_buffers,
        )
    }

    fn flush(&mut self) {
        if let Some(buffer) = self
            .glyph_texture_manager
            .flush(&self.device, &mut self.index_buffer)
        {
            self.command_buffers.push(buffer);
        }
        let buffers = mem::take(&mut self.command_buffers);
        self.queue.submit(buffers);
    }
}
