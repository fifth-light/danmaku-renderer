use std::collections::HashMap;

use cairo::{Context, Format, ImageSurface, Operator, SurfacePattern};
use cosmic_text::{CacheKey, FontSystem, Placement, SwashCache, SwashContent};

use crate::{
    danmaku::DanmakuTime,
    layout::DanmakuPosition,
    manager::DanmakuTimeChunk,
    worker::{DanmakuParam, RenderCache},
};

use super::RendererParam;

#[derive(Clone)]
struct ImageData {
    format: Format,
    width: u32,
    height: u32,
    stride: u32,
    data: Vec<u8>,
}

#[derive(Clone)]
enum GlyphImage {
    Mask(ImageData),
    Color(ImageData),
}

pub struct StrideGlyphCache {
    images: HashMap<CacheKey, Option<(GlyphImage, Placement)>>,
    swash_cache: SwashCache,
    danmaku_param: DanmakuParam,
}

impl StrideGlyphCache {
    pub fn new(param: DanmakuParam) -> Self {
        Self {
            images: Default::default(),
            swash_cache: SwashCache::new(),
            danmaku_param: param,
        }
    }
}

impl RenderCache for StrideGlyphCache {
    fn new_param(&mut self, new_param: DanmakuParam) {
        if (new_param.font_size != self.danmaku_param.font_size)
            || (new_param.font_attrs != self.danmaku_param.font_attrs)
        {
            self.swash_cache.image_cache.clear();
            self.swash_cache.outline_command_cache.clear();
        }
        self.danmaku_param = new_param;
    }

    fn prepare(&mut self, font_system: &mut FontSystem, chunk: &DanmakuTimeChunk) {
        for glyph in chunk.glyph_ids() {
            if !self.images.contains_key(glyph) {
                self.images.insert(
                    *glyph,
                    Self::generate(&mut self.swash_cache, font_system, *glyph),
                );
            }
        }
    }
}

impl StrideGlyphCache {
    fn generate(
        swash_cache: &mut SwashCache,
        font_system: &mut FontSystem,
        glyph: CacheKey,
    ) -> Option<(GlyphImage, Placement)> {
        let image = swash_cache.get_image_uncached(font_system, glyph)?;
        let width = image.placement.width;
        let height = image.placement.height;

        let surface = match image.content {
            SwashContent::SubpixelMask => todo!(),
            SwashContent::Mask => {
                assert_eq!(image.data.len(), (width * height) as usize);
                let format = Format::A8;
                let stride = format.stride_for_width(width).unwrap() as u32;
                let mut data: Vec<u8> = Vec::with_capacity((stride * height) as usize);
                let padding = stride - width;

                let mut data_offset = 0usize;
                for _ in 0..height {
                    for _ in 0..width {
                        let alpha = image.data[data_offset];
                        data.push(alpha);
                        data_offset += 1;
                    }
                    data.resize(data.len() + padding as usize, 0);
                }

                GlyphImage::Mask(ImageData {
                    format,
                    width,
                    height,
                    stride,
                    data,
                })
            }
            SwashContent::Color => {
                assert_eq!(image.data.len(), (width * height * 4) as usize);
                let format = Format::ARgb32;
                let stride = format.stride_for_width(width).unwrap() as u32;
                let mut data: Vec<u8> = Vec::with_capacity((stride * height) as usize);
                let padding = stride - width;

                let mut data_offset = 0usize;
                for _ in 0..height {
                    for _ in 0..width {
                        for _ in 0..4 {
                            let alpha = image.data[data_offset];
                            data.push(alpha);
                            data_offset += 1;
                        }
                    }
                    data.resize(data.len() + padding as usize, 0);
                }

                GlyphImage::Color(ImageData {
                    format,
                    width,
                    height,
                    stride,
                    data,
                })
            }
        };
        Some((surface, image.placement))
    }

    fn get(&self, glyph: CacheKey) -> Option<&(GlyphImage, Placement)> {
        self.images.get(&glyph).and_then(|item| item.as_ref())
    }
}

enum CairoGlyphImage {
    Mask(SurfacePattern),
    Color(ImageSurface),
}

impl TryFrom<GlyphImage> for CairoGlyphImage {
    type Error = cairo::Error;

    fn try_from(value: GlyphImage) -> Result<Self, Self::Error> {
        match value {
            GlyphImage::Mask(image) => {
                let surface = ImageSurface::create_for_data(
                    image.data,
                    image.format,
                    image.width as i32,
                    image.height as i32,
                    image.stride as i32,
                )?;
                let pattern = SurfacePattern::create(surface);
                Ok(CairoGlyphImage::Mask(pattern))
            }
            GlyphImage::Color(image) => {
                let surface = ImageSurface::create_for_data(
                    image.data,
                    image.format,
                    image.width as i32,
                    image.height as i32,
                    image.stride as i32,
                )?;
                Ok(CairoGlyphImage::Color(surface))
            }
        }
    }
}

#[derive(Default)]
pub struct CairoGlyphCache {
    surfaces: HashMap<CacheKey, Option<(CairoGlyphImage, Placement)>>,
}

impl CairoGlyphCache {
    fn get(
        &mut self,
        cache: &StrideGlyphCache,
        glyph: CacheKey,
    ) -> Option<&(CairoGlyphImage, Placement)> {
        self.surfaces
            .entry(glyph)
            .or_insert_with(|| {
                let image = cache.get(glyph);
                image.and_then(|(image, placement)| {
                    let image = image.clone().try_into().ok()?;
                    Some((image, *placement))
                })
            })
            .as_ref()
    }
}

pub struct CairoRenderer {
    renderer_param: RendererParam,
}

impl CairoRenderer {
    pub fn new(renderer_param: RendererParam) -> Self {
        CairoRenderer { renderer_param }
    }

    pub fn update_renderer_param(&mut self, param: RendererParam) {
        self.renderer_param = param;
    }

    pub fn draw_chunk(
        &self,
        param: &DanmakuParam,
        chunk: &DanmakuTimeChunk,
        glyph_cache: &StrideGlyphCache,
        cario_glyph_cache: &mut CairoGlyphCache,
        context: &Context,
        now_time: DanmakuTime,
    ) -> Result<(), cairo::Error> {
        context.save()?;

        context.set_operator(Operator::Source);
        let opacity = self.renderer_param.opacity as f64;

        for item in &chunk.items {
            let time = item.item.time;
            if now_time < time || now_time - time >= param.lifetime {
                continue;
            }

            context.save()?;
            match item.position {
                DanmakuPosition::Scroll(pos) => {
                    let progress = (now_time.as_millis() as f64 - time.as_millis() as f64)
                        / param.lifetime.as_millis() as f64;
                    let x = (param.screen_size.0 as f64)
                        - (param.screen_size.0 as f64 + item.item.width() as f64) * progress;
                    let y = (pos as f64 + 1.0) * param.line_height as f64;
                    context.translate(x, y);
                }
                DanmakuPosition::Top(pos) | DanmakuPosition::Bottom(pos) => {
                    let x = (param.screen_size.0 as f64 - item.item.width() as f64) / 2.0;
                    let y = match item.position {
                        DanmakuPosition::Top(_) => (pos as f64 + 1.0) * param.line_height as f64,
                        DanmakuPosition::Bottom(_) => {
                            param.screen_size.1 as f64 - pos as f64 * param.line_height as f64
                        }
                        _ => unreachable!(),
                    };
                    context.translate(x, y);
                }
            };
            context.translate(0.0, -(item.item.layout_line.max_descent as f64));

            for glyph in &item.item.physical_glyphs {
                let image = cario_glyph_cache.get(glyph_cache, glyph.cache_key);
                if let Some((image, placement)) = image {
                    context.save()?;

                    context.translate(glyph.x as f64, glyph.y as f64);
                    context.translate(placement.left as f64, -placement.top as f64);

                    match image {
                        CairoGlyphImage::Mask(mask) => {
                            let r = (item.item.color.r() as f64) / 255.0;
                            let g = (item.item.color.g() as f64) / 255.0;
                            let b = (item.item.color.b() as f64) / 255.0;
                            context.set_source_rgba(r, g, b, opacity);
                            context.rectangle(
                                0.0,
                                0.0,
                                placement.width as f64,
                                placement.height as f64,
                            );
                            context.mask(mask)?;
                        }
                        CairoGlyphImage::Color(color) => {
                            context.set_source_surface(color, 0.0, 0.0)?;
                            context.paint()?;
                        }
                    }

                    context.restore()?;
                }
            }

            context.restore()?;
        }

        context.restore()?;
        Ok(())
    }
}
