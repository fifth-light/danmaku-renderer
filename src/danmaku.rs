use std::{
    cmp::Ordering,
    fmt::{self, Debug, Formatter},
    ops::Sub,
    time::Duration,
};

#[derive(PartialEq, Eq, Clone, Copy)]
pub struct DanmakuTime(u32);

impl DanmakuTime {
    pub fn abs_diff(&self, other: &DanmakuTime) -> Duration {
        let diff = self.0.abs_diff(other.0);
        Duration::from_millis(diff as u64)
    }

    pub fn from_millis(milis: u32) -> Self {
        DanmakuTime(milis)
    }

    pub fn as_millis(&self) -> u32 {
        self.0
    }

    pub fn seconds(&self) -> u32 {
        self.0 / 1000
    }

    pub fn millis(&self) -> u16 {
        (self.0 % 1000) as u16
    }
}

impl Ord for DanmakuTime {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.cmp(&other.0)
    }
}

impl PartialOrd for DanmakuTime {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Debug for DanmakuTime {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let minutes = self.seconds() / 60;
        let seconds = self.seconds() % 60;
        write!(f, "{:02}:{:02}.{:03}", minutes, seconds, self.millis())
    }
}

impl Sub for DanmakuTime {
    type Output = Duration;

    fn sub(self, rhs: Self) -> Self::Output {
        let milliseconds = self.0 - rhs.0;
        Duration::from_millis(milliseconds as u64)
    }
}

impl Sub for &DanmakuTime {
    type Output = Duration;

    fn sub(self, rhs: Self) -> Self::Output {
        *self - *rhs
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct DanmakuColor(u32);

impl DanmakuColor {
    pub fn from_code(rgb: u32) -> Self {
        assert!(rgb >> 24 == 0);
        DanmakuColor(rgb)
    }

    pub fn from_code_cast(rgb: u32) -> Self {
        DanmakuColor(rgb & 0xFFFFFF)
    }

    pub fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        DanmakuColor((r as u32) << 16 | (g as u32) << 8 | (b as u32))
    }

    pub fn r(&self) -> u8 {
        (self.0 >> 16) as u8
    }

    pub fn g(&self) -> u8 {
        (self.0 >> 8) as u8
    }

    pub fn b(&self) -> u8 {
        self.0 as u8
    }

    pub fn code(&self) -> u32 {
        self.0
    }
}

impl Debug for DanmakuColor {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "#{:02X}{:02X}{:02X}", self.r(), self.g(), self.b())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DanmakuType {
    Scroll,
    Top,
    Bottom,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DanmakuSize {
    Small,
    Regular,
    Large,
}

#[derive(Clone, Debug)]
pub struct Danmaku {
    pub time: DanmakuTime,
    pub r#type: DanmakuType,
    pub size: DanmakuSize,
    pub color: DanmakuColor,
    pub content: String,
}
