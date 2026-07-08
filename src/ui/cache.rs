// Copied from: https://github.com/zed-industries/zed/blob/6b733d105896a20924bd4aba87bd7baa20b62ac6/crates/gpui/examples/image_gallery.rs#L134

use futures::FutureExt;
use gpui::{
    App, AppContext as _, Asset as _, AssetLogger, Context, ElementId, Entity, ImageAssetLoader,
    ImageCache, ImageCacheProvider, Window, hash,
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
    fn provide(&mut self, window: &mut Window, cx: &mut App) -> gpui::AnyImageCache {
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
    cache: HashMap<u64, gpui::ImageCacheItem>,
}

impl SimpleLruCache {
    pub fn new(id: ElementId, max_items: usize, cx: &mut Context<Self>) -> Self {
        tracing::debug!(cache = %id, max_items, "cache created");

        cx.on_release({
            let id = id.clone();
            move |simple_cache, cx| {
                tracing::debug!(cache = %id, "cache released");
                for (_, mut item) in std::mem::take(&mut simple_cache.cache) {
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
        resource: &gpui::Resource,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Result<Arc<gpui::RenderImage>, gpui::ImageCacheError>> {
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

            return item.get();
        }

        tracing::trace!(cache = %self.id, hash, size, "miss");

        let fut = AssetLogger::<ImageAssetLoader>::load(resource.clone(), cx);
        let task = cx.background_executor().spawn(fut).shared();
        if self.usages.len() == self.max_items {
            let oldest = self.usages.pop().unwrap();
            let mut image = self
                .cache
                .remove(&oldest)
                .expect("cache and usages must be in sync");

            tracing::debug!(cache = %self.id, evicted = oldest, "evict");

            if let Some(Ok(image)) = image.get() {
                cx.drop_image(image, Some(window));
            }
        }
        self.cache
            .insert(hash, gpui::ImageCacheItem::Loading(task.clone()));
        self.usages.insert(0, hash);

        tracing::trace!(cache = %self.id, hash, size = self.cache.len(), "insert");

        let entity = window.current_view();
        window
            .spawn(cx, {
                async move |cx| {
                    _ = task.await;
                    cx.on_next_frame(move |_, cx| {
                        cx.notify(entity);
                    });
                }
            })
            .detach();

        None
    }
}
