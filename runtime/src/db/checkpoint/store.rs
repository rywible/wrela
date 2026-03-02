pub trait CheckpointStore: Send + Sync {
    type Error: std::fmt::Display + Send + Sync + 'static;

    fn put_object(&self, key: &str, data: &[u8]) -> Result<(), Self::Error>;
    fn get_object(&self, key: &str) -> Result<Vec<u8>, Self::Error>;
    fn list_prefix(&self, prefix: &str) -> Result<Vec<String>, Self::Error>;
    fn delete_object(&self, key: &str) -> Result<(), Self::Error>;
    fn exists(&self, key: &str) -> Result<bool, Self::Error>;
}
