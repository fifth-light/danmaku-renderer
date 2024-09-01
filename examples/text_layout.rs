use std::fs::OpenOptions;

use cairo::{Context, Format, ImageSurface, SurfacePattern};
use cosmic_text::{
    fontdb::Database, Attrs, AttrsList, Family, FontSystem, ShapeBuffer, ShapeLine, Shaping,
    SwashCache, SwashContent, Wrap,
};

fn main() {
    let mut database = Database::new();
    database.load_system_fonts();
    database.set_serif_family("Noto Serif CJK JP");
    database.set_sans_serif_family("Noto Sans CJK JP");
    let locale = "ja";

    let mut font_system = FontSystem::new_with_locale_and_db(String::from(locale), database);

    let mut shape_buffer = ShapeBuffer::default();
    let mut cache = SwashCache::new();

    let mut attrs = Attrs::new();
    attrs.family = Family::Serif;
    let attrs_list = AttrsList::new(attrs);

    let shape_line = ShapeLine::new(
        &mut font_system,
        "甘い砂糖を頂戴",
        &attrs_list,
        Shaping::Advanced,
        8,
    );

    let mut layout_lines = Vec::new();
    shape_line.layout_to_buffer(
        &mut shape_buffer,
        64.0,
        None,
        Wrap::None,
        None,
        &mut layout_lines,
        None,
    );

    let surface = ImageSurface::create(Format::ARgb32, 768, 128).unwrap();
    let surface_context = Context::new(&surface).unwrap();

    for layout_line in layout_lines {
        let line_height = 64.0;

        surface_context.save().unwrap();
        surface_context.set_source_rgba(1.0, 1.0, 0.0, 1.0);
        surface_context.rectangle(0.0, 0.0, layout_line.w.into(), line_height);
        surface_context.fill().unwrap();
        surface_context.restore().unwrap();

        surface_context.translate(0.0, line_height);

        for glyph in layout_line.glyphs {
            let physical = glyph.physical((0.0, 0.0), 1.0);

            surface_context.save().unwrap();
            surface_context.translate(physical.x as f64, physical.y as f64);

            let image = cache.get_image_uncached(&mut font_system, physical.cache_key);
            if let Some(image) = image {
                let width = image.placement.width;
                let height = image.placement.height;

                surface_context.translate(image.placement.left as f64, -image.placement.top as f64);

                surface_context.save().unwrap();
                surface_context.set_source_rgba(1.0, 0.0, 0.0, 1.0);
                surface_context.rectangle(0.0, 0.0, width as f64, height as f64);
                surface_context.set_line_width(1.0);
                surface_context.stroke().unwrap();
                surface_context.restore().unwrap();

                match image.content {
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

                        let mask_surface = ImageSurface::create_for_data(
                            data,
                            format,
                            width as i32,
                            height as i32,
                            stride as i32,
                        )
                        .unwrap();
                        let mask_pattern = SurfacePattern::create(mask_surface);

                        surface_context.rectangle(0.0, 0.0, width as f64, height as f64);
                        surface_context.mask(mask_pattern).unwrap();
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

                        let glyph_surface = ImageSurface::create_for_data(
                            data,
                            format,
                            width as i32,
                            height as i32,
                            stride as i32,
                        )
                        .unwrap();

                        surface_context
                            .set_source_surface(glyph_surface, 0.0, 0.0)
                            .unwrap();
                        surface_context.paint().unwrap();
                    }
                };
            }
            surface_context.restore().unwrap();
        }
    }

    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open("result.png")
        .unwrap();
    surface.write_to_png(&mut file).unwrap();
}
