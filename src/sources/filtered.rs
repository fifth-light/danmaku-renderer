use std::{borrow::Borrow, marker::PhantomData};

use crate::{
    danmaku::{Danmaku, DanmakuTime},
    filter::DanmakuFilter,
};

use super::DanmakuSource;

pub struct FilteredDanmakuSource<Source: DanmakuSource, Filter: DanmakuFilter> {
    source: Source,
    filter: Filter,
}

impl<Source: DanmakuSource, Filter: DanmakuFilter> FilteredDanmakuSource<Source, Filter> {
    pub fn new(source: Source, filter: Filter) -> Self {
        FilteredDanmakuSource { source, filter }
    }
}

struct FilteredDanmakuSourceIterator<'a, Item, Filter, FilterItem>
where
    Item: Borrow<Danmaku>,
    Filter: Borrow<FilterItem>,
    FilterItem: DanmakuFilter,
{
    iterator: Box<dyn Iterator<Item = Item> + 'a>,
    is_ended: bool,
    filter: Filter,
    filter_item: PhantomData<FilterItem>,
}

impl<'a, Item, Filter, FilterItem> FilteredDanmakuSourceIterator<'a, Item, Filter, FilterItem>
where
    Item: Borrow<Danmaku>,
    Filter: Borrow<FilterItem>,
    FilterItem: DanmakuFilter,
{
    fn new(iterator: Box<dyn Iterator<Item = Item> + 'a>, filter: Filter) -> Self {
        FilteredDanmakuSourceIterator {
            iterator,
            is_ended: false,
            filter,
            filter_item: PhantomData,
        }
    }
}

impl<'a, Item, Filter, FilterItem> Iterator
    for FilteredDanmakuSourceIterator<'a, Item, Filter, FilterItem>
where
    Item: Borrow<Danmaku> + 'a,
    Filter: Borrow<FilterItem>,
    FilterItem: DanmakuFilter,
{
    type Item = Item;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.is_ended {
                return None;
            }
            let item = self.iterator.next();
            match item {
                Some(item) => {
                    let filter = self.filter.borrow();
                    if filter.borrow().is_filtered(&item.borrow().content) {
                        continue;
                    } else {
                        return Some(item);
                    }
                }
                None => {
                    self.is_ended = true;
                    return None;
                }
            }
        }
    }
}

impl<Source, Filter> DanmakuSource for FilteredDanmakuSource<Source, Filter>
where
    Source: DanmakuSource,
    Filter: DanmakuFilter + 'static,
{
    fn get_range(
        &mut self,
        start_included: DanmakuTime,
        end_excluded: DanmakuTime,
    ) -> Box<dyn Iterator<Item = &'_ Danmaku> + '_> {
        let iter = self.source.get_range(start_included, end_excluded);
        let iter = FilteredDanmakuSourceIterator::<&'_ Danmaku, &'_ Filter, Filter>::new(
            iter,
            &self.filter,
        );
        Box::new(iter)
    }

    fn get_all(&mut self) -> Box<dyn Iterator<Item = &'_ Danmaku> + '_> {
        let iter = self.source.get_all();
        let iter = FilteredDanmakuSourceIterator::<&'_ Danmaku, &'_ Filter, Filter>::new(
            iter,
            &self.filter,
        );
        Box::new(iter)
    }

    fn into_all(self) -> Box<dyn Iterator<Item = Danmaku>> {
        let iter = self.source.into_all();
        let iter = FilteredDanmakuSourceIterator::<Danmaku, Filter, Filter>::new(iter, self.filter);
        Box::new(iter)
    }
}
