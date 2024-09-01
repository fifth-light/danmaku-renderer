use super::DanmakuFilter;

pub struct MergeFilter {
    filters: Vec<Box<dyn DanmakuFilter>>,
}

impl MergeFilter {
    pub fn new(filters: Vec<Box<dyn DanmakuFilter>>) -> Self {
        MergeFilter { filters }
    }
}

impl DanmakuFilter for MergeFilter {
    fn is_filtered(&self, content: &str) -> bool {
        for filter in &self.filters {
            if filter.is_filtered(content) {
                return true;
            }
        }
        false
    }
}
