use async_trait::async_trait;
use std::hash::Hash;

#[async_trait]
pub trait Cache<K, V>: Send + Sync
where
    K: Hash + Eq + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    async fn get(&self, key: &K) -> Option<V>;
    async fn insert(&self, key: K, value: V);
    async fn remove(&self, key: &K);
    #[allow(dead_code)] // Used through trait objects
    async fn clear(&self);
    #[allow(dead_code)] // Used through trait objects
    async fn contains_key(&self, key: &K) -> bool;
}
