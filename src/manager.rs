use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    sync::Arc,
    time::Duration,
};

use cosmic_text::{
    AttrsList, CacheKey, FontSystem, LayoutLine, PhysicalGlyph, ShapeBuffer, ShapeLine, Shaping,
    Wrap,
};

use crate::{
    danmaku::{Danmaku, DanmakuColor, DanmakuSize, DanmakuTime, DanmakuType},
    layout::{DanmakuPosition, DanmakuTrackState, LayoutMode},
    sources::DanmakuSource,
};

#[derive(Debug)]
pub struct LayoutedDanmakuItem {
    pub layout_line: LayoutLine,
    pub physical_glyphs: Vec<PhysicalGlyph>,
    pub time: DanmakuTime,
    pub color: DanmakuColor,
    pub r#type: DanmakuType,
    pub size: DanmakuSize,
}

impl LayoutedDanmakuItem {
    fn new(
        font_system: &mut FontSystem,
        shape_buffer: &mut ShapeBuffer,
        attrs: &AttrsList,
        font_size: f32,
        danmaku: &Danmaku,
    ) -> Option<LayoutedDanmakuItem> {
        let shape_line = ShapeLine::new_in_buffer(
            shape_buffer,
            font_system,
            &danmaku.content,
            attrs,
            Shaping::Advanced,
            2,
        );
        let mut lines = Vec::new();
        shape_line.layout_to_buffer(
            shape_buffer,
            font_size,
            None,
            Wrap::None,
            None,
            &mut lines,
            None,
        );
        lines.into_iter().nth(0).map(|line| {
            let physical_glyphs = line
                .glyphs
                .iter()
                .map(|glyph| glyph.physical((0.0, 0.0), 1.0))
                .collect();
            LayoutedDanmakuItem {
                layout_line: line,
                physical_glyphs,
                time: danmaku.time,
                color: danmaku.color,
                r#type: danmaku.r#type,
                size: danmaku.size,
            }
        })
    }

    pub fn width(&self) -> u32 {
        self.layout_line.w.ceil() as u32
    }
}

#[derive(Debug)]
pub struct PositionedDanmakuItem {
    pub item: LayoutedDanmakuItem,
    pub position: DanmakuPosition,
}

#[derive(Debug)]
pub struct DanmakuTimeChunk {
    pub base_state_index: u32,
    pub index: u32,
    pub items: Vec<PositionedDanmakuItem>,
    glyph_ids: BTreeSet<CacheKey>,
}

impl DanmakuTimeChunk {
    pub fn glyph_ids(&self) -> impl Iterator<Item = &CacheKey> {
        self.glyph_ids.iter()
    }
}

pub struct DanmakuTimeChunkProvider {
    lifetime: Duration,
    font_size: f32,
    font_attrs: AttrsList,
    screen_size: (u32, u32),
    line_height: u32,
    layout_mode: LayoutMode,
    source: Box<dyn DanmakuSource + Send>,
    states: BTreeMap<u32, (u32, DanmakuTrackState)>,
    chunks: BTreeMap<u32, Arc<DanmakuTimeChunk>>,
}

impl DanmakuTimeChunkProvider {
    pub fn new(
        lifetime: Duration,
        font_size: f32,
        font_attrs: AttrsList,
        screen_size: (u32, u32),
        line_height: u32,
        layout_mode: LayoutMode,
        source: Box<dyn DanmakuSource + Send>,
    ) -> Self {
        DanmakuTimeChunkProvider {
            lifetime,
            font_size,
            font_attrs,
            screen_size,
            line_height,
            layout_mode,
            source,
            states: BTreeMap::new(),
            chunks: BTreeMap::new(),
        }
    }

    pub fn source(self) -> Box<dyn DanmakuSource + Send> {
        self.source
    }

    pub fn lifetime(&self) -> Duration {
        self.lifetime
    }

    fn generate_chunk(
        &mut self,
        font_system: &mut FontSystem,
        shape_buffer: &mut ShapeBuffer,
        base_state_index: u32,
        base_state: &mut DanmakuTrackState,
        index: u32,
    ) -> Arc<DanmakuTimeChunk> {
        let lifetime = self.lifetime.as_millis() as u32;
        let start_millis = lifetime * index;
        let end_millis = start_millis + lifetime;
        let start_time = DanmakuTime::from_millis(start_millis);
        let end_time = DanmakuTime::from_millis(end_millis);

        let mut items = Vec::new();
        let mut glyph_ids = BTreeSet::new();
        for danmaku in self.source.get_range(start_time, end_time) {
            if let Some(layouted) = LayoutedDanmakuItem::new(
                font_system,
                shape_buffer,
                &self.font_attrs,
                self.font_size,
                danmaku,
            ) {
                if let Some(position) = base_state.insert((&layouted).into()) {
                    for glyph in &layouted.physical_glyphs {
                        glyph_ids.insert(glyph.cache_key);
                    }

                    let item = PositionedDanmakuItem {
                        item: layouted,
                        position,
                    };
                    items.push(item);
                }
            }
        }

        Arc::new(DanmakuTimeChunk {
            base_state_index,
            index,
            items,
            glyph_ids,
        })
    }

    pub fn get_chunk(
        &mut self,
        font_system: &mut FontSystem,
        shape_buffer: &mut ShapeBuffer,
        base_state_index: Option<u32>,
        index: u32,
    ) -> Result<Arc<DanmakuTimeChunk>, Box<dyn Error>> {
        if let Some(chunk) = self.chunks.get(&index) {
            if let Some(base_state_index) = base_state_index {
                if chunk.base_state_index == base_state_index {
                    return Ok(chunk.clone());
                }
            } else {
                return Ok(chunk.clone());
            }
        }

        let base_state = if index != 0 {
            let index = index - 1;
            self.states.remove(&index)
        } else {
            None
        };
        let (base_state_index, mut base_state_item) = base_state.unwrap_or_else(|| {
            (
                index,
                DanmakuTrackState::new(
                    self.layout_mode,
                    self.screen_size,
                    self.line_height,
                    self.lifetime,
                ),
            )
        });

        let chunk = self.generate_chunk(
            font_system,
            shape_buffer,
            base_state_index,
            &mut base_state_item,
            index,
        );

        self.states
            .insert(index, (base_state_index, base_state_item));
        self.chunks.insert(index, chunk.clone());

        Ok(chunk)
    }
}

#[cfg(test)]
mod test {
    use std::{fs::File, io::Read, time::Duration};

    use cosmic_text::{Attrs, AttrsList, FontSystem, ShapeBuffer};

    use crate::{
        layout::LayoutMode, manager::DanmakuTimeChunkProvider, sources::bilibili::parse_proto,
    };

    #[test]
    fn test_chunk_generate() {
        let mut font_system = FontSystem::new();
        let mut shape_buffer = ShapeBuffer::default();
        let attr = Attrs::new();
        let attrs = AttrsList::new(attr);

        let mut file = File::open("test/1176840.bin").unwrap();
        let mut content = Vec::new();
        file.read_to_end(&mut content).unwrap();
        let source = parse_proto(&content).unwrap();

        let mut provider = DanmakuTimeChunkProvider::new(
            Duration::from_secs(8),
            28.0,
            attrs,
            (1280, 720),
            32,
            LayoutMode::ShowAll,
            Box::new(source),
        );

        provider
            .get_chunk(&mut font_system, &mut shape_buffer, None, 0)
            .unwrap();
        for i in 1..10 {
            provider
                .get_chunk(&mut font_system, &mut shape_buffer, Some(0), i)
                .unwrap();
        }
        let chunk = provider
            .get_chunk(&mut font_system, &mut shape_buffer, Some(0), 10)
            .unwrap();
        println!("{:?}", chunk);
    }
}
