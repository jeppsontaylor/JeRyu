#[tokio::main]
async fn main() {
    let db = vgit::state::Db::open().await.unwrap();
    let manager = vgit::cache::CacheManager;
    manager.gc_disk_cache().await.unwrap();
    println!("Done");
}
