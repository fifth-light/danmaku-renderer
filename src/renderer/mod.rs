#[cfg(feature = "renderer-cairo")]
pub mod cairo;
pub mod noop;
#[cfg(feature = "renderer-wgpu")]
pub mod wgpu;

#[derive(Clone)]
pub struct RendererParam {
    pub opacity: f32,
}
