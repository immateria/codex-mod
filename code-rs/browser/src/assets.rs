use crate::Result;
use crate::config::ImageFormat;
use chrono::DateTime;
use chrono::Duration;
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageRef {
    pub path: String,
    pub mime: String,
    pub width: u32,
    pub height: u32,
    pub ttl_ms: u64,
    pub created_at: DateTime<Utc>,
}

pub struct AssetManager {
    base_dir: PathBuf,
    assets: Arc<RwLock<HashMap<String, ImageRef>>>,
}

impl AssetManager {
    pub async fn new() -> Result<Self> {
        let session_id = Uuid::new_v4().to_string();
        let base_dir = PathBuf::from("/tmp/codex/browser").join(&session_id);

        fs::create_dir_all(&base_dir).await?;

        Ok(Self {
            base_dir,
            assets: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    pub async fn store_screenshot(
        &self,
        data: &[u8],
        format: ImageFormat,
        width: u32,
        height: u32,
        ttl_ms: u64,
    ) -> Result<ImageRef> {
        let filename = format!(
            "{}.{}",
            Uuid::new_v4(),
            match format {
                ImageFormat::Png => "png",
                ImageFormat::Webp => "webp",
            }
        );

        let path = self.base_dir.join(&filename);
        fs::write(&path, data).await?;

        let mime = match format {
            ImageFormat::Png => "image/png",
            ImageFormat::Webp => "image/webp",
        }
        .to_string();

        let image_ref = ImageRef {
            path: path.to_string_lossy().to_string(),
            mime,
            width,
            height,
            ttl_ms,
            created_at: Utc::now(),
        };

        let mut assets = self.assets.write().await;
        assets.insert(filename, image_ref.clone());

        Ok(image_ref)
    }

    pub async fn store_screenshots(
        &self,
        screenshots: Vec<crate::page::Screenshot>,
        ttl_ms: u64,
    ) -> Result<Vec<ImageRef>> {
        let mut refs = Vec::new();

        for screenshot in screenshots {
            let image_ref = self
                .store_screenshot(
                    &screenshot.data,
                    screenshot.format,
                    screenshot.width,
                    screenshot.height,
                    ttl_ms,
                )
                .await?;
            refs.push(image_ref);
        }

        Ok(refs)
    }

    pub async fn cleanup_expired(&self) -> Result<()> {
        let now = Utc::now();
        let mut assets = self.assets.write().await;
        let mut to_remove = Vec::new();

        for (key, asset) in assets.iter() {
            let age = now - asset.created_at;
            if age > Duration::milliseconds(asset.ttl_ms as i64) {
                to_remove.push(key.clone());
                let _ = fs::remove_file(&asset.path).await;
            }
        }

        for key in to_remove {
            assets.remove(&key);
        }

        Ok(())
    }

    pub async fn cleanup_all(&self) -> Result<()> {
        if self.base_dir.exists() {
            fs::remove_dir_all(&self.base_dir).await?;
        }
        Ok(())
    }

    pub fn get_session_dir(&self) -> &Path {
        &self.base_dir
    }
}

impl Drop for AssetManager {
    fn drop(&mut self) {
        let base_dir = self.base_dir.clone();
        tokio::spawn(async move {
            let _ = fs::remove_dir_all(&base_dir).await;
        });
    }
}
