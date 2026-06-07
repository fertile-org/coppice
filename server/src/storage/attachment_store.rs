use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Clone)]
pub struct AttachmentStore {
    root: PathBuf,
    max_bytes: u64,
}

impl AttachmentStore {
    pub fn new(root: impl Into<PathBuf>, max_bytes: u64) -> Self {
        Self {
            root: root.into(),
            max_bytes,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn max_bytes(&self) -> u64 {
        self.max_bytes
    }

    pub fn save(
        &self,
        id: Uuid,
        filename: &str,
        _content_type: &str,
        bytes: &[u8],
    ) -> anyhow::Result<PathBuf> {
        if bytes.len() as u64 > self.max_bytes {
            anyhow::bail!("file too large");
        }
        let dir = self.root.join(id.to_string());
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(sanitize_filename(filename));
        std::fs::write(&path, bytes)?;
        Ok(path)
    }
}

fn sanitize_filename(filename: &str) -> String {
    let base = Path::new(filename)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file");
    base.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect()
}
