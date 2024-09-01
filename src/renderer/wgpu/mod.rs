mod config;
mod copy;
mod glyph_atlas;
mod glyph_manager;
mod glyph_shadow;
mod index_buffer;
mod render_cache;
mod renderer;
mod timestamp;
mod vertex_buffer;

pub use render_cache::WgpuRenderCache;
pub use renderer::WgpuRenderer;
pub use vertex_buffer::VertexBuffer as WgpuVertexBuffer;
pub use wgpu;

use crate::worker::{WorkerBuffer, WorkerManager};

pub type WgpuWorkerBuffer = WorkerBuffer<WgpuRenderCache, WgpuVertexBuffer>;
pub type WgpuWorkerManager = WorkerManager<WgpuRenderCache, WgpuVertexBuffer>;
