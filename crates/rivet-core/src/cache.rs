//! A small cache of rendered pages with a memory budget, so scrolling back
//! to a page doesn't render it again, while memory use stays bounded.

use lru::LruCache;

use crate::{DocId, RenderedPage, Rotation};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Key {
    pub doc: DocId,
    pub page: u32,
    /// Scale × 1000, so nearly identical scales share an entry.
    pub scale_milli: u32,
    pub rotation: Rotation,
}

impl Key {
    pub fn new(doc: DocId, page: u32, scale: f32, rotation: Rotation) -> Self {
        Self {
            doc,
            page,
            scale_milli: (scale * 1000.0).round() as u32,
            rotation,
        }
    }
}

pub(crate) struct RenderCache {
    entries: LruCache<Key, RenderedPage>,
    bytes: usize,
    budget: usize,
}

impl RenderCache {
    pub fn new(budget_bytes: usize) -> Self {
        Self {
            entries: LruCache::unbounded(),
            bytes: 0,
            budget: budget_bytes,
        }
    }

    pub fn get(&mut self, key: &Key) -> Option<RenderedPage> {
        self.entries.get(key).cloned()
    }

    pub fn insert(&mut self, key: Key, page: RenderedPage) {
        let size = page.rgba.len();
        if size > self.budget {
            return; // A single huge page (extreme zoom) isn't worth caching.
        }
        if let Some(old) = self.entries.put(key, page) {
            self.bytes -= old.rgba.len();
        }
        self.bytes += size;
        while self.bytes > self.budget {
            match self.entries.pop_lru() {
                Some((_, evicted)) => self.bytes -= evicted.rgba.len(),
                None => break,
            }
        }
    }

    /// Drops every cached page of a document (when it is closed).
    pub fn remove_doc(&mut self, doc: DocId) {
        let keys: Vec<Key> = self
            .entries
            .iter()
            .filter(|(k, _)| k.doc == doc)
            .map(|(k, _)| *k)
            .collect();
        for key in keys {
            if let Some(page) = self.entries.pop(&key) {
                self.bytes -= page.rgba.len();
            }
        }
    }

    #[cfg(test)]
    pub fn bytes(&self) -> usize {
        self.bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(bytes: usize) -> RenderedPage {
        RenderedPage {
            width: 1,
            height: 1,
            rgba: vec![0u8; bytes].into(),
        }
    }

    #[test]
    fn stays_within_budget_and_evicts_oldest() {
        let mut cache = RenderCache::new(100);
        let key = |p| Key::new(1, p, 1.0, Rotation::None);
        cache.insert(key(0), page(40));
        cache.insert(key(1), page(40));
        cache.get(&key(0)); // page 0 is now the most recently used
        cache.insert(key(2), page(40));
        assert!(cache.bytes() <= 100);
        assert!(cache.get(&key(0)).is_some());
        assert!(
            cache.get(&key(1)).is_none(),
            "least recently used page is evicted"
        );
    }

    #[test]
    fn removes_a_documents_pages() {
        let mut cache = RenderCache::new(1000);
        cache.insert(Key::new(1, 0, 1.0, Rotation::None), page(10));
        cache.insert(Key::new(2, 0, 1.0, Rotation::None), page(10));
        cache.remove_doc(1);
        assert_eq!(cache.bytes(), 10);
    }
}
