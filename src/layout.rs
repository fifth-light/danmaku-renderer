use std::time::Duration;

use crate::{
    danmaku::{DanmakuTime, DanmakuType},
    manager::LayoutedDanmakuItem,
};

#[derive(Debug)]
pub enum DanmakuPosition {
    Scroll(usize),
    Top(usize),
    Bottom(usize),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LayoutMode {
    NoOverlap(u32),
    ShowAll,
}

#[derive(Debug)]
pub struct DanmakuItem {
    width: u32,
    time: DanmakuTime,
    r#type: DanmakuType,
}

impl From<&LayoutedDanmakuItem> for DanmakuItem {
    fn from(value: &LayoutedDanmakuItem) -> Self {
        DanmakuItem {
            width: value.width(),
            time: value.time,
            r#type: value.r#type,
        }
    }
}

#[derive(Debug)]
struct StaticDanmakuTrackState {
    tracks: Vec<Option<DanmakuItem>>,
    lifetime: Duration,
    index: usize,
}

impl StaticDanmakuTrackState {
    fn new(tracks: usize, lifetime: Duration) -> Self {
        StaticDanmakuTrackState {
            tracks: (0..tracks).map(|_| None).collect(),
            lifetime,
            index: 0,
        }
    }

    fn clear_expired(&mut self, now_time: DanmakuTime) {
        for track in self.tracks.iter_mut() {
            if let Some(item) = track {
                if now_time - item.time > self.lifetime {
                    *track = None;
                }
            }
        }
    }

    fn find_track(&self, mode: LayoutMode) -> Option<usize> {
        if self.tracks.is_empty() {
            return None;
        }
        let track = self.find_empty_track();
        match mode {
            LayoutMode::NoOverlap(_) => track,
            LayoutMode::ShowAll => Some(track.unwrap_or_else(|| self.index % self.tracks.len())),
        }
    }

    fn find_empty_track(&self) -> Option<usize> {
        self.tracks.iter().position(|track| track.is_none())
    }

    fn insert(&mut self, track: usize, item: DanmakuItem) {
        self.tracks[track] = Some(item);
        self.index += 1
    }
}

#[derive(Debug)]
struct ScrollDanmakuTrack {
    latest_danmaku_item: Option<DanmakuItem>,
}

#[derive(Debug)]
struct ScrollDanmakuTrackState {
    tracks: Vec<ScrollDanmakuTrack>,
    lifetime: Duration,
    screen_width: u32,
    index: usize,
}

impl ScrollDanmakuTrack {
    fn new() -> Self {
        ScrollDanmakuTrack {
            latest_danmaku_item: None,
        }
    }

    fn clear_expired(&mut self, lifetime: Duration, now_time: DanmakuTime) {
        if let Some(latest_danmaku_item) = &self.latest_danmaku_item {
            let passed_time = now_time - latest_danmaku_item.time;
            if passed_time > lifetime {
                self.latest_danmaku_item = None
            }
        }
    }

    fn will_overlap(&self, screen_width: u32, lifetime: Duration, item: &DanmakuItem) -> bool {
        if let Some(last_item) = &self.latest_danmaku_item {
            let lifetime: f64 = lifetime.as_millis() as f64;

            let speed_last: f64 = (screen_width + last_item.width) as f64 / lifetime;
            let speed_current: f64 = (screen_width + item.width) as f64 / lifetime;

            let time_last = last_item.time.as_millis();
            let time_current = item.time.as_millis();

            let distance_last =
                speed_last * (time_current - time_last) as f64 - last_item.width as f64;

            if distance_last < 0.0 {
                return true;
            }
            if speed_last > speed_current {
                return false;
            }

            let time_to_reach = distance_last / (speed_current - speed_last);
            let time_current = screen_width as f64 / speed_current;
            time_to_reach < time_current
        } else {
            false
        }
    }

    fn insert(&mut self, item: DanmakuItem) {
        self.latest_danmaku_item = Some(item);
    }
}

impl ScrollDanmakuTrackState {
    fn new(tracks: usize, screen_width: u32, lifetime: Duration) -> Self {
        let tracks = (0..tracks).map(|_| ScrollDanmakuTrack::new()).collect();
        ScrollDanmakuTrackState {
            tracks,
            screen_width,
            lifetime,
            index: 0,
        }
    }

    fn clear_expired(&mut self, now_time: DanmakuTime) {
        self.tracks
            .iter_mut()
            .for_each(|track| track.clear_expired(self.lifetime, now_time))
    }

    fn find_empty_track(&self, item: &DanmakuItem) -> Option<usize> {
        self.tracks
            .iter()
            .position(|track| !track.will_overlap(self.screen_width, self.lifetime, item))
    }

    fn find_track(&self, item: &DanmakuItem, mode: LayoutMode) -> Option<usize> {
        if self.tracks.is_empty() {
            return None;
        }
        let track = self.find_empty_track(item);
        match mode {
            LayoutMode::NoOverlap(_) => track,
            LayoutMode::ShowAll => Some(track.unwrap_or_else(|| self.index % self.tracks.len())),
        }
    }

    fn insert(&mut self, track: usize, item: DanmakuItem) {
        self.index += 1;
        self.tracks[track].insert(item)
    }
}

#[derive(Debug)]
pub struct DanmakuTrackState {
    mode: LayoutMode,
    top: StaticDanmakuTrackState,
    bottom: StaticDanmakuTrackState,
    scroll: ScrollDanmakuTrackState,
}

impl DanmakuTrackState {
    pub fn new(
        mode: LayoutMode,
        screen_size: (u32, u32),
        line_height: u32,
        lifetime: Duration,
    ) -> Self {
        let total_tracks = (screen_size.1 / line_height) as usize;
        let (scroll_tracks, static_tracks) = match mode {
            LayoutMode::NoOverlap(percent) => {
                let tracks = total_tracks * percent as usize / 100;
                (tracks, tracks.min(total_tracks / 2))
            }
            LayoutMode::ShowAll => (total_tracks, total_tracks),
        };
        DanmakuTrackState {
            mode,
            top: StaticDanmakuTrackState::new(static_tracks, lifetime),
            bottom: StaticDanmakuTrackState::new(static_tracks, lifetime),
            scroll: ScrollDanmakuTrackState::new(scroll_tracks, screen_size.0, lifetime),
        }
    }

    pub fn insert(&mut self, item: DanmakuItem) -> Option<DanmakuPosition> {
        match item.r#type {
            DanmakuType::Scroll => {
                self.scroll.clear_expired(item.time);
                if let Some(track) = self.scroll.find_track(&item, self.mode) {
                    self.scroll.insert(track, item);
                    Some(DanmakuPosition::Scroll(track))
                } else {
                    None
                }
            }
            DanmakuType::Top | DanmakuType::Bottom => {
                let state = match item.r#type {
                    DanmakuType::Top => &mut self.top,
                    DanmakuType::Bottom => &mut self.bottom,
                    _ => unreachable!(),
                };
                state.clear_expired(item.time);
                if let Some(track) = state.find_track(self.mode) {
                    let result = match item.r#type {
                        DanmakuType::Top => DanmakuPosition::Top(track),
                        DanmakuType::Bottom => DanmakuPosition::Bottom(track),
                        _ => unreachable!(),
                    };
                    state.insert(track, item);
                    Some(result)
                } else {
                    None
                }
            }
            DanmakuType::Unknown => None,
        }
    }
}
