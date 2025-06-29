use async_trait::async_trait;
use moka::future::Cache as MokaCache;
use std::hash::Hash;
use std::time::Duration;
use crate::ports::Cache;

pub struct MokaCacheAdapter<K, V> {
    inner: MokaCache<K, V>,
}

impl<K, V> MokaCacheAdapter<K, V>
where
    K: Hash + Eq + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    pub fn new(ttl_seconds: u64, max_capacity: u64) -> Self {
        let cache = MokaCache::builder()
            .time_to_live(Duration::from_secs(ttl_seconds))
            .max_capacity(max_capacity)
            .build();

        Self { inner: cache }
    }

    pub fn with_default_settings() -> Self {
        Self::new(300, 10_000) // 5 minutes TTL, 10k max items
    }
}

#[async_trait]
impl<K, V> Cache<K, V> for MokaCacheAdapter<K, V>
where
    K: Hash + Eq + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    async fn get(&self, key: &K) -> Option<V> {
        self.inner.get(key).await
    }

    async fn insert(&self, key: K, value: V) {
        self.inner.insert(key, value).await;
    }

    async fn remove(&self, key: &K) {
        self.inner.remove(key).await;
    }

    async fn clear(&self) {
        self.inner.invalidate_all();
    }

    async fn contains_key(&self, key: &K) -> bool {
        self.inner.contains_key(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_test;

    #[tokio::test]
    async fn test_cache_operations() {
        let cache = MokaCacheAdapter::<String, i32>::with_default_settings();
        
        // Test insert and get
        cache.insert("key1".to_string(), 42).await;
        assert_eq!(cache.get(&"key1".to_string()).await, Some(42));
        
        // Test contains_key
        assert!(cache.contains_key(&"key1".to_string()).await);
        assert!(!cache.contains_key(&"nonexistent".to_string()).await);
        
        // Test remove
        cache.remove(&"key1".to_string()).await;
        assert_eq!(cache.get(&"key1".to_string()).await, None);
        
        // Test clear
        cache.insert("key2".to_string(), 100).await;
        cache.insert("key3".to_string(), 200).await;
        cache.clear().await;
        assert_eq!(cache.get(&"key2".to_string()).await, None);
        assert_eq!(cache.get(&"key3".to_string()).await, None);
    }
}