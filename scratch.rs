#[tokio::main]
async fn main() {
    let db = jeryu::state::Db::open().await.unwrap();
    let manager = jeryu::cache::CacheManager;
    manager.gc_disk_cache().await.unwrap();
    println!("Done");
}
