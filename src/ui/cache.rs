// Adapted from: https://github.com/zed-industries/zed/blob/main/crates/gpui/examples/image_gallery.rs (SimpleLruCache)

use gpui_kit::gpui::{
    App, AppContext as _, Context, ElementId, Entity, ImageCache, ImageCacheItem,
    ImageCacheProvider, Window, hash,
};
use std::{collections::HashMap, sync::Arc};

pub fn simple_lru_cache(id: impl Into<ElementId>, max_items: usize) -> SimpleLruCacheProvider {
    SimpleLruCacheProvider {
        id: id.into(),
        max_items,
    }
}

pub struct SimpleLruCacheProvider {
    id: ElementId,
    max_items: usize,
}

impl ImageCacheProvider for SimpleLruCacheProvider {
    fn provide(&mut self, window: &mut Window, cx: &mut App) -> gpui_kit::gpui::AnyImageCache {
        let id = self.id.clone();
        let max_items = self.max_items;

        window
            .with_global_id(self.id.clone(), |global_id, window| {
                window.with_element_state::<Entity<SimpleLruCache>, _>(
                    global_id,
                    |lru_cache, _window| {
                        let mut lru_cache = lru_cache.unwrap_or_else(|| {
                            cx.new(|cx| SimpleLruCache::new(id.clone(), max_items, cx))
                        });
                        if lru_cache.read(cx).max_items != max_items {
                            lru_cache = cx.new(|cx| SimpleLruCache::new(id, max_items, cx));
                        }
                        (lru_cache.clone(), lru_cache)
                    },
                )
            })
            .into()
    }
}

pub struct SimpleLruCache {
    id: ElementId,
    max_items: usize,
    usages: Vec<u64>,
    cache: HashMap<u64, ImageCacheItem>,
}

impl SimpleLruCache {
    pub fn new(id: ElementId, max_items: usize, cx: &mut Context<Self>) -> Self {
        tracing::debug!(cache = %id, max_items, "cache created");

        cx.on_release({
            let id = id.clone();
            move |simple_cache, cx| {
                tracing::debug!(cache = %id, "cache released");
                for (_, item) in std::mem::take(&mut simple_cache.cache) {
                    if let Some(Ok(image)) = item.get() {
                        cx.drop_image(image, None);
                    }
                }
            }
        })
        .detach();

        Self {
            id,
            max_items,
            usages: Vec::with_capacity(max_items),
            cache: HashMap::with_capacity(max_items),
        }
    }
}

impl ImageCache for SimpleLruCache {
    fn load(
        &mut self,
        resource: &gpui_kit::gpui::Resource,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Result<Arc<gpui_kit::gpui::RenderImage>, gpui_kit::gpui::ImageCacheError>> {
        assert_eq!(self.usages.len(), self.cache.len());
        assert!(self.cache.len() <= self.max_items);

        let hash = hash(resource);
        let size = self.cache.len();

        if let Some(item) = self.cache.get_mut(&hash) {
            let current_ix = self
                .usages
                .iter()
                .position(|item| *item == hash)
                .expect("cache and usages must stay in sync");
            self.usages.remove(current_ix);
            self.usages.insert(0, hash);

            tracing::trace!(cache = %self.id, hash, size, "hit");

            return item.use_image(window);
        }

        tracing::trace!(cache = %self.id, hash, size, "miss");

        if self.usages.len() == self.max_items {
            let oldest = self.usages.pop().unwrap();
            let image = self
                .cache
                .remove(&oldest)
                .expect("cache and usages must be in sync");

            tracing::debug!(cache = %self.id, evicted = oldest, "evict");

            if let Some(Ok(image)) = image.get() {
                cx.drop_image(image, Some(window));
            }
        }
        let item = ImageCacheItem::new(resource, cx);

        // Subscribes the current view to be notified when loading completes
        let result = item.use_image(window);

        self.cache.insert(hash, item);
        self.usages.insert(0, hash);

        tracing::trace!(cache = %self.id, hash, size = self.cache.len(), "insert");

        result
    }
}
