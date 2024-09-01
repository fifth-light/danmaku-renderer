pub mod bilibili;
pub mod filtered;

use crate::danmaku::{Danmaku, DanmakuTime};

pub trait DanmakuSource {
    fn get_range<'a>(
        &'a mut self,
        start_included: DanmakuTime,
        end_excluded: DanmakuTime,
    ) -> Box<dyn Iterator<Item = &'a Danmaku> + 'a>;

    fn get_all(&mut self) -> Box<dyn Iterator<Item = &'_ Danmaku> + '_>;

    fn into_all(self) -> Box<dyn Iterator<Item = Danmaku>>;
}

pub struct VecDanmakuSource(Vec<Danmaku>);

impl VecDanmakuSource {
    pub fn new(mut vec: Vec<Danmaku>) -> Self {
        vec.sort_unstable_by(|a, b| a.time.cmp(&b.time));
        VecDanmakuSource(vec)
    }

    fn find_index(&self, time: DanmakuTime) -> Result<usize, usize> {
        self.0.binary_search_by(|item| item.time.cmp(&time))
    }
}

struct VecDanmakuSourceIterator<'a> {
    source: &'a [Danmaku],
    index: usize,
    end_excluded: DanmakuTime,
}

impl<'a> VecDanmakuSourceIterator<'a> {
    fn new(source: &'a [Danmaku], index: usize, end_excluded: DanmakuTime) -> Self {
        VecDanmakuSourceIterator {
            source,
            index,
            end_excluded,
        }
    }
}

impl<'a> Iterator for VecDanmakuSourceIterator<'a> {
    type Item = &'a Danmaku;

    fn next(&mut self) -> Option<Self::Item> {
        let item = &self.source[self.index];
        if item.time >= self.end_excluded {
            None
        } else {
            self.index += 1;
            Some(item)
        }
    }
}

impl DanmakuSource for VecDanmakuSource {
    fn get_range(
        &mut self,
        start_included: DanmakuTime,
        end_excluded: DanmakuTime,
    ) -> Box<dyn Iterator<Item = &'_ Danmaku> + '_> {
        let start = self
            .find_index(start_included)
            .unwrap_or_else(|index| index);

        Box::new(VecDanmakuSourceIterator::new(&self.0, start, end_excluded))
    }

    fn get_all(&mut self) -> Box<dyn Iterator<Item = &'_ Danmaku> + '_> {
        Box::new(self.0.iter())
    }

    fn into_all(self) -> Box<dyn Iterator<Item = Danmaku>> {
        Box::new(self.0.into_iter())
    }
}

#[cfg(test)]
mod test {
    // TODO
}
