use anyhow::Result;

use super::*;

impl SmartCache {
    /// Store a blob in CAS with Scratch -> Hash -> Fsync -> Rename safety pattern
    pub async fn atomic_store_cas(data: &[u8], digest: &str) -> Result<()> {
        let cas_dir = crate::config::data_dir().join("cas");
        tokio::fs::create_dir_all(&cas_dir).await?;

        let path = cas_dir.join(digest);
        if path.exists() {
            return Ok(());
        }

        let scratch_path = cas_dir.join(format!("{}.tmp", digest));

        use tokio::io::AsyncWriteExt;
        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&scratch_path)
            .await?;

        use sha2::Digest;
        let result = sha2::Sha256::digest(data);
        let computed_digest = hex::encode(result);
        if computed_digest != digest {
            return Err(CacheError::CasHashMismatch(digest.to_string(), computed_digest).into());
        }

        file.write_all(data).await?;
        file.sync_all().await?;

        tokio::fs::rename(scratch_path, path).await?;
        Ok(())
    }
}
