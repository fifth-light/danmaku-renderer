use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use cosmic_text::{Attrs, AttrsList, Family, FontSystem, ShapeBuffer, Weight};
use danmaku_renderer::{
    danmaku::DanmakuTime,
    layout::LayoutMode,
    manager::DanmakuTimeChunk,
    renderer::{
        cairo::{CairoGlyphCache, CairoRenderer, StrideGlyphCache},
        RendererParam,
    },
    sources::bilibili::parse_xml_from_file,
    worker::{DanmakuParam, WorkerBuffer, WorkerManager, WorkerState},
};
use gtk::glib::{timeout_add_local, ControlFlow};
use gtk::prelude::*;
use gtk::{glib, Application, ApplicationWindow, DrawingArea, Fixed, Settings};
use gtk4::{self as gtk, Orientation};
use log::warn;

fn get_window_size(window: &ApplicationWindow) -> (u32, u32) {
    let width = window.size(Orientation::Horizontal) as u32;
    let height = window.size(Orientation::Vertical) as u32;
    (width, height)
}

fn build_param(screen_size: (u32, u32)) -> DanmakuParam {
    let attrs = Attrs::new();
    attrs.family(Family::SansSerif);
    attrs.weight(Weight::BOLD);
    DanmakuParam {
        screen_size,
        lifetime: Duration::from_secs(8),
        font_size: 28.0,
        line_height: 32,
        font_attrs: AttrsList::new(attrs),
        layout_mode: LayoutMode::NoOverlap(25),
        shadow_size: 0,
        shadow_weight: 0.0,
    }
}

fn main() -> glib::ExitCode {
    env_logger::init();

    let application = Application::builder()
        .application_id("top.fifthlight.danmaku.render.cairo")
        .build();

    application.connect_activate(move |app: &Application| {
        let path = Path::new("test/1176840.xml");
        let source = Box::new(parse_xml_from_file(path).unwrap());

        let settings = Settings::default().unwrap();
        settings.set_gtk_application_prefer_dark_theme(true);

        let window = ApplicationWindow::builder()
            .application(app)
            .default_width(1280)
            .default_height(720)
            .build();

        let header_bar = Fixed::builder().build();
        window.set_titlebar(Some(&header_bar));

        let area = DrawingArea::builder().build();

        let param = build_param(get_window_size(&window));
        let source = source;
        let cache = StrideGlyphCache::new(param.clone());
        let buffer = Arc::new(Mutex::new(
            WorkerBuffer::<StrideGlyphCache, DanmakuTimeChunk>::new(cache),
        ));
        let font_system = FontSystem::new();
        let shape_buffer = ShapeBuffer::default();
        let state = WorkerState {
            buffer: buffer.clone(),
            font_system,
            shape_buffer,
            source,
        };
        let mut worker = WorkerManager::new(param.clone(), state);
        let param = Arc::new(Mutex::new(param));

        if worker.request(None, 0).is_err() {
            warn!("Failed to request initial chunk");
        }

        let start = Instant::now();
        let mut glyph_cache = CairoGlyphCache::default();
        let worker = Arc::new(Mutex::new(Some(worker)));
        let area_param = param.clone();
        let area_worker = worker.clone();

        let renderer = CairoRenderer::new(RendererParam { opacity: 1.0 });
        area.set_draw_func(move |_, context, _, _| {
            let param = area_param.lock().unwrap();

            let now_time = start.elapsed();
            let now_time = DanmakuTime::from_millis(now_time.as_millis() as u32);
            let lifetime = param.lifetime.as_millis() as u32;
            let index = now_time.as_millis() / lifetime;

            let buffer = buffer.lock().unwrap();

            let mut worker = area_worker.lock().unwrap();
            let worker = worker.as_mut().unwrap();

            let mut begin_state_chunk = None;
            if let Some((previous, current)) = buffer.acquire_index(index) {
                begin_state_chunk = Some(previous.base_state_index);
                if let Err(err) = renderer.draw_chunk(
                    &param,
                    previous,
                    &buffer.cache,
                    &mut glyph_cache,
                    context,
                    now_time,
                ) {
                    warn!("Draw failed: {}", err)
                }
                if let Err(err) = renderer.draw_chunk(
                    &param,
                    current,
                    &buffer.cache,
                    &mut glyph_cache,
                    context,
                    now_time,
                ) {
                    warn!("Draw failed: {}", err)
                }
            } else {
                warn!("No chunk for index: {}", index);
                if worker.request(Some(0), index).is_err() {
                    warn!("Failed to request chunk {}", index);
                }
            }

            if buffer.should_request_worker(index) {
                if let Some(begin_state_chunk) = begin_state_chunk {
                    if worker.request(Some(begin_state_chunk), index).is_err() {
                        warn!("Failed to request chunk {}", index);
                    }
                }
            }
        });
        let resize_worker = worker.clone();
        area.connect_resize(move |area, width, height| {
            if !area.is_visible() {
                return;
            }
            if width == 0 || height == 0 {
                return;
            }
            let new_param = build_param((width as u32, height as u32));
            let mut param = param.lock().unwrap();
            *param = new_param.clone();
            let mut worker = resize_worker.lock().unwrap();
            if let Some(worker) = worker.as_mut() {
                worker.change_param(new_param).unwrap();
            }
        });
        area.connect_destroy(move |_| {
            let mut worker = worker.lock().unwrap();
            if let Some(worker) = worker.take() {
                drop(worker)
            }
        });

        window.set_child(Some(&area));
        timeout_add_local(Duration::from_millis(10), move || {
            area.queue_draw();
            ControlFlow::Continue
        });

        window.present();
    });

    application.run()
}
