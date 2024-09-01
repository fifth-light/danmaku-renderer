use crate::worker::{DanmakuParam, RenderCache};

#[derive(Default)]
pub struct NoopRenderCache;

impl RenderCache for NoopRenderCache {
    fn new_param(&mut self, _new_param: DanmakuParam) {}

    fn prepare<'a>(
        &mut self,
        _font_system: &mut cosmic_text::FontSystem,
        _chunk: &crate::manager::DanmakuTimeChunk,
    ) {
    }
}
