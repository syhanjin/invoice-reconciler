use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use tokio::io::AsyncReadExt;
use uuid::Uuid;

use crate::error::{AppError, AppResult};

const SUPPORTED_EXTENSIONS: &[&str] = &["bmp", "gif", "jpeg", "jpg", "png", "tif", "tiff", "webp"];

pub fn normalized_extension(path: &Path) -> AppResult<String> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| AppError::InvalidInput(format!("图片缺少扩展名：{}", path.display())))?;
    SUPPORTED_EXTENSIONS
        .contains(&extension.as_str())
        .then_some(())
        .ok_or_else(|| AppError::InvalidInput(format!("不支持的图片格式：{extension}")))?;
    Ok(extension)
}

pub async fn sha256_file(path: &Path) -> AppResult<String> {
    let mut file = tokio::fs::File::open(path).await?;
    let metadata = file.metadata().await?;
    if !metadata.is_file() {
        return Err(AppError::InvalidInput(format!(
            "导入路径不是文件：{}",
            path.display()
        )));
    }

    let mut digest = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    let mut header = Vec::with_capacity(16);
    loop {
        let read = file.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        if header.len() < 16 {
            let remaining = 16 - header.len();
            header.extend_from_slice(&buffer[..read.min(remaining)]);
        }
        digest.update(&buffer[..read]);
    }
    validate_image_content(path, &header)?;
    Ok(format!("{:x}", digest.finalize()))
}

fn validate_image_content(path: &Path, header: &[u8]) -> AppResult<()> {
    let kind = infer::get(header).ok_or_else(|| {
        AppError::InvalidInput(format!("文件内容不是支持的图片：{}", path.display()))
    })?;
    let mime = kind.mime_type();
    let supported = matches!(
        mime,
        "image/bmp" | "image/gif" | "image/jpeg" | "image/png" | "image/tiff" | "image/webp"
    );
    supported.then_some(()).ok_or_else(|| {
        AppError::InvalidInput(format!("文件内容不是支持的图片：{}", path.display()))
    })
}

pub async fn copy_managed_image(
    source: &Path,
    images_dir: &Path,
    id: Uuid,
    extension: &str,
) -> AppResult<PathBuf> {
    tokio::fs::create_dir_all(images_dir).await?;
    let file_name = format!("{id}.{extension}");
    let destination = images_dir.join(&file_name);
    let temporary = images_dir.join(format!(".{id}.{extension}.tmp"));

    tokio::fs::copy(source, &temporary).await?;
    if let Err(error) = tokio::fs::rename(&temporary, &destination).await {
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err(error.into());
    }
    Ok(PathBuf::from("images").join(file_name))
}

pub fn resolve_managed_path(app_data_dir: &Path, relative: &str) -> AppResult<PathBuf> {
    let relative = Path::new(relative);
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
        || !relative.starts_with("images")
    {
        return Err(AppError::File("受管图片路径越界".into()));
    }
    Ok(app_data_dir.join(relative))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_and_normalizes_extensions() {
        assert_eq!(
            normalized_extension(Path::new("order.JPEG")).unwrap(),
            "jpeg"
        );
        assert!(normalized_extension(Path::new("order.svg")).is_err());
    }

    #[test]
    fn managed_paths_cannot_escape_image_directory() {
        assert!(resolve_managed_path(Path::new("/app"), "images/id.png").is_ok());
        assert!(resolve_managed_path(Path::new("/app"), "../secret").is_err());
    }

    #[test]
    fn rejects_non_image_content_with_image_extension() {
        assert!(validate_image_content(Path::new("fake.png"), b"plain text").is_err());
    }
}
