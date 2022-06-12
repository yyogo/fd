use crate::walk::DirEntry;

pub trait Filter: Send + Sync + Sized {
    /// Whether the entry should be skipped or not.
    fn should_skip(&self, entry: &DirEntry) -> bool;

    fn chain<F: Filter>(self, other: F) -> ChainedFilter<Self, F> {
        ChainedFilter(self, other)
    }
}

pub struct ChainedFilter<F1: Filter, F2: Filter>(F1, F2);

impl<F1: Filter, F2: Filter> Filter for ChainedFilter<F1, F2> {
    fn should_skip(&self, entry: &DirEntry) -> bool {
        self.0.should_skip(entry) || self.1.should_skip(entry)
    }
}

impl<F> Filter for Option<F>
where
    F: Filter,
{
    fn should_skip(&self, entry: &DirEntry) -> bool {
        self.as_ref().map_or(false, |f| f.should_skip(entry))
    }
}
