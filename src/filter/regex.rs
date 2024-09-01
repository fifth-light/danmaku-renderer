use regex::Regex;

use super::DanmakuFilter;

pub struct RegexFilter {
    regex: Regex,
}

impl RegexFilter {
    pub fn new(regex: Regex) -> Self {
        RegexFilter { regex }
    }
}

impl DanmakuFilter for RegexFilter {
    fn is_filtered(&self, content: &str) -> bool {
        self.regex.is_match(content)
    }
}
