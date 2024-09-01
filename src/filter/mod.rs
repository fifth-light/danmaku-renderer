mod merge;
mod simple;

pub use merge::MergeFilter;
pub use simple::SimpleFilter;

#[cfg(feature = "regex")]
mod regex;
#[cfg(feature = "regex")]
pub use regex::RegexFilter;

pub trait DanmakuFilter {
    fn is_filtered(&self, content: &str) -> bool;
}
