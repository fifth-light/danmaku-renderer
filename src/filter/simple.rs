use super::DanmakuFilter;

pub struct SimpleFilter {
    keyword: String,
}

impl SimpleFilter {
    pub fn new(keyword: String) -> Self {
        SimpleFilter { keyword }
    }
}

impl DanmakuFilter for SimpleFilter {
    fn is_filtered(&self, content: &str) -> bool {
        content.contains(&self.keyword)
    }
}
