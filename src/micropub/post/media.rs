use std::{io, sync::Arc};

use chrono::Datelike;
use object_store::{ObjectStoreExt, PutPayload, aws::{AmazonS3, AmazonS3Builder}, path::Path};
use thiserror::Error;
use tokio::io::AsyncReadExt;
use tracing::{info, instrument};

use crate::{AppState, micropub::post::UploadedFile};

#[derive(Debug, Error)]
pub enum MediaError {
  #[error("object store error: {0}")]
  ObjectStore(#[from] object_store::Error),
  #[error("async-tempfile error: {0}")]
  TempFile(#[from] async_tempfile::Error),
  #[error("io error: {0}")]
  Io(#[from] io::Error)
}

pub fn get_s3(state: &Arc<AppState>) -> Result<AmazonS3, MediaError> {
  let s3_config = &state.config.micropub.media.s3;
  AmazonS3Builder::new()
    .with_access_key_id(&s3_config.access_key_id)
    .with_secret_access_key(&s3_config.secret_access_key)
    .with_region(s3_config.region.clone().unwrap_or_default())
    .with_bucket_name(&s3_config.bucket)
    .with_endpoint(&s3_config.endpoint)
    .build()
    .map_err(|e| MediaError::ObjectStore(e))
}

/// Uploads a media file to backing storage and returns its permalink
#[instrument(skip(state))]
pub async fn persist_file(state: &Arc<AppState>, s3: &AmazonS3, file: &UploadedFile) -> Result<String, MediaError> {
  let now = chrono::Utc::now();
  let object_key = Path::from(format!("{}/{:02}/{:02}/{}", now.year(), now.month(), now.day(), uuid::Uuid::new_v4().as_hyphenated().to_string()));

  info!("reading file to buf");
  let mut buf = Vec::new();
  file.file
    .open_ro()
    .await?
    .read_to_end(&mut buf)
    .await?;

  info!("uploading buf to s3");
  let payload = PutPayload::from(buf);
  s3.put(&object_key, payload).await?;

  Ok(state.config.micropub.media.get_media_url(&object_key.to_string()))
}