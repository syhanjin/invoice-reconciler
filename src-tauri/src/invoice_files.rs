use std::path::{Component, Path, PathBuf};

use base64::{Engine, engine::general_purpose::STANDARD};
use lopdf::Document;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::error::{AppError, AppResult};

pub const MAX_INVOICE_FILES: usize = 500;
pub const MAX_BASE64_BYTES: u64 = 10 * 1024 * 1024;
pub const MAX_PDF_PAGES: u32 = 30;

#[derive(Clone, Debug)]
pub struct InspectedInvoiceFile {
    pub original_file_name: String,
    pub extension: String,
    pub media_type: String,
    pub size_bytes: u64,
    pub pdf_pages: u32,
    pub hash: String,
}

pub async fn inspect_invoice_file(path: &Path) -> AppResult<InspectedInvoiceFile> {
    let metadata = tokio::fs::metadata(path).await.map_err(|error| {
        AppError::InvalidInput(format!("无法读取文件 {}：{error}", path.display()))
    })?;
    if !metadata.is_file() {
        return Err(AppError::InvalidInput(format!(
            "{} 不是文件",
            path.display()
        )));
    }
    let original_file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| AppError::InvalidInput("文件名不是有效 UTF-8".into()))?
        .to_owned();
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let expected = match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "pdf" => "application/pdf",
        _ => {
            return Err(AppError::InvalidInput(
                "仅支持 PNG、JPG、JPEG 和 PDF 发票文件".into(),
            ));
        }
    };
    let bytes = tokio::fs::read(path).await?;
    let encoded_size = (bytes.len() as u64).div_ceil(3) * 4;
    if encoded_size > MAX_BASE64_BYTES {
        return Err(AppError::InvalidInput(
            "Base64 编码后超过腾讯云 10 MiB 限制".into(),
        ));
    }
    let detected = infer::get(&bytes)
        .map(|kind| kind.mime_type())
        .unwrap_or_default();
    if detected != expected {
        return Err(AppError::InvalidInput(format!(
            "文件内容与 .{extension} 扩展名不一致"
        )));
    }
    let pdf_pages = if extension == "pdf" {
        let document = Document::load_mem(&bytes)
            .map_err(|error| AppError::InvalidInput(format!("PDF 无法解析：{error}")))?;
        let pages = document.get_pages().len() as u32;
        if pages == 0 || pages > MAX_PDF_PAGES {
            return Err(AppError::InvalidInput(
                "PDF 页数必须在 1 到 30 页之间".into(),
            ));
        }
        pages
    } else {
        let size = imagesize::blob_size(&bytes)
            .map_err(|error| AppError::InvalidInput(format!("图片无法解析：{error}")))?;
        if !(20..=10_000).contains(&size.width) || !(20..=10_000).contains(&size.height) {
            return Err(AppError::InvalidInput(
                "图片单边像素必须在 20 到 10000 之间".into(),
            ));
        }
        0
    };
    Ok(InspectedInvoiceFile {
        original_file_name,
        extension,
        media_type: expected.into(),
        size_bytes: metadata.len(),
        pdf_pages,
        hash: format!("{:x}", Sha256::digest(&bytes)),
    })
}

pub async fn copy_invoice_source(
    source: &Path,
    app_data: &Path,
    id: Uuid,
    extension: &str,
) -> AppResult<PathBuf> {
    let relative = PathBuf::from("invoices")
        .join("sources")
        .join(format!("{id}.{extension}"));
    let destination = app_data.join(&relative);
    if let Some(parent) = destination.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::copy(source, &destination).await?;
    Ok(relative)
}

pub async fn copy_invoice_preview(
    source: &Path,
    app_data: &Path,
    invoice_id: &str,
    extension: &str,
) -> AppResult<PathBuf> {
    let relative = PathBuf::from("invoices")
        .join("previews")
        .join(format!("{invoice_id}.{extension}"));
    let destination = app_data.join(&relative);
    if let Some(parent) = destination.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::copy(source, &destination).await?;
    Ok(relative)
}

pub async fn write_cut_image(
    encoded: &str,
    app_data: &Path,
    invoice_id: &str,
) -> AppResult<PathBuf> {
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|error| AppError::InvalidOcr(format!("切图 Base64 无效：{error}")))?;
    let kind = infer::get(&bytes).ok_or_else(|| AppError::InvalidOcr("切图格式未知".into()))?;
    let extension = match kind.mime_type() {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        _ => return Err(AppError::InvalidOcr("腾讯返回了不支持的切图格式".into())),
    };
    let relative = PathBuf::from("invoices")
        .join("previews")
        .join(format!("{invoice_id}.{extension}"));
    let destination = app_data.join(&relative);
    if let Some(parent) = destination.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&destination, bytes).await?;
    Ok(relative)
}

pub async fn crop_invoice_polygon(
    source: &Path,
    polygon: &serde_json::Value,
    app_data: &Path,
    invoice_id: &str,
) -> AppResult<PathBuf> {
    let bytes = tokio::fs::read(source).await?;
    let image = image::load_from_memory(&bytes)
        .map_err(|error| AppError::File(format!("无法解码混贴发票图片：{error}")))?;
    let points = ["LeftTop", "RightTop", "RightBottom", "LeftBottom"]
        .into_iter()
        .filter_map(|name| {
            let value = polygon.get(name)?;
            Some((
                value.get("X")?.as_u64()? as u32,
                value.get("Y")?.as_u64()? as u32,
            ))
        })
        .collect::<Vec<_>>();
    if points.len() != 4 {
        return Err(AppError::InvalidOcr("混贴发票缺少有效 Polygon 坐标".into()));
    }
    let left = points
        .iter()
        .map(|point| point.0)
        .min()
        .unwrap_or(0)
        .min(image.width());
    let top = points
        .iter()
        .map(|point| point.1)
        .min()
        .unwrap_or(0)
        .min(image.height());
    let right = points
        .iter()
        .map(|point| point.0)
        .max()
        .unwrap_or(0)
        .min(image.width());
    let bottom = points
        .iter()
        .map(|point| point.1)
        .max()
        .unwrap_or(0)
        .min(image.height());
    if right <= left || bottom <= top {
        return Err(AppError::InvalidOcr("混贴发票 Polygon 坐标超出图片".into()));
    }
    let relative = PathBuf::from("invoices")
        .join("previews")
        .join(format!("{invoice_id}.png"));
    let destination = app_data.join(&relative);
    if let Some(parent) = destination.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    image
        .crop_imm(left, top, right - left, bottom - top)
        .save_with_format(&destination, image::ImageFormat::Png)
        .map_err(|error| AppError::File(format!("无法保存混贴发票切图：{error}")))?;
    Ok(relative)
}

pub fn extract_pdf_pages(
    source: &Path,
    app_data: &Path,
    invoice_id: &str,
    retained_pages: &[u32],
) -> AppResult<PathBuf> {
    let relative = PathBuf::from("invoices")
        .join("previews")
        .join(format!("{invoice_id}.pdf"));
    let destination = app_data.join(&relative);
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut document =
        Document::load(source).map_err(|error| AppError::File(format!("PDF 无法解析：{error}")))?;
    let all_pages = document.get_pages().keys().copied().collect::<Vec<_>>();
    let remove = all_pages
        .into_iter()
        .filter(|page| !retained_pages.contains(page))
        .collect::<Vec<_>>();
    document.delete_pages(&remove);
    document.prune_objects();
    document.renumber_objects();
    document
        .save(&destination)
        .map_err(|error| AppError::File(format!("无法保存拆分 PDF：{error}")))?;
    Ok(relative)
}

pub fn resolve_invoice_path(app_data: &Path, relative: &str) -> AppResult<PathBuf> {
    let relative_path = Path::new(relative);
    if relative_path.is_absolute()
        || relative_path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(AppError::File("受管发票路径无效".into()));
    }
    let resolved = app_data.join(relative_path);
    if !resolved.starts_with(app_data) {
        return Err(AppError::File("发票路径超出应用数据目录".into()));
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_pdf(path: &Path, page_count: usize) {
        use lopdf::{Object, Stream, dictionary};

        let mut document = Document::with_version("1.5");
        let pages_id = document.new_object_id();
        let contents_id = document.add_object(Stream::new(dictionary! {}, Vec::new()));
        let page_ids = (0..page_count)
            .map(|_| {
                document.add_object(dictionary! {
                    "Type" => "Page",
                    "Parent" => pages_id,
                    "Contents" => contents_id,
                })
            })
            .collect::<Vec<_>>();
        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => page_ids.iter().copied().map(Object::Reference).collect::<Vec<_>>(),
                "Count" => page_count as i64,
                "Resources" => dictionary! {},
                "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            }),
        );
        let catalog_id = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        document.trailer.set("Root", catalog_id);
        document.save(path).unwrap();
    }

    #[tokio::test]
    async fn rejects_extension_magic_mismatch() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("fake.png");
        tokio::fs::write(&path, b"not an image").await.unwrap();
        assert!(matches!(
            inspect_invoice_file(&path).await,
            Err(AppError::InvalidInput(_))
        ));
    }

    #[tokio::test]
    async fn crops_polygon_to_an_independent_preview() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source.png");
        image::RgbaImage::from_pixel(100, 80, image::Rgba([1, 2, 3, 255]))
            .save(&source)
            .unwrap();
        let polygon = serde_json::json!({
            "LeftTop":{"X":10,"Y":20}, "RightTop":{"X":60,"Y":20},
            "RightBottom":{"X":60,"Y":60}, "LeftBottom":{"X":10,"Y":60}
        });

        let relative = crop_invoice_polygon(&source, &polygon, directory.path(), "cropped")
            .await
            .unwrap();
        let preview = image::open(directory.path().join(relative)).unwrap();
        assert_eq!((preview.width(), preview.height()), (50, 40));
    }

    #[test]
    fn extracts_only_the_selected_pdf_pages() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source.pdf");
        write_pdf(&source, 3);

        let relative = extract_pdf_pages(&source, directory.path(), "split", &[2, 3]).unwrap();
        let preview = Document::load(directory.path().join(relative)).unwrap();
        assert_eq!(preview.get_pages().len(), 2);
    }

    #[test]
    fn managed_invoice_paths_cannot_escape_app_data() {
        let app_data = Path::new("/tmp/invoice-reconciler-test-app-data");
        assert!(resolve_invoice_path(app_data, "invoices/previews/a.pdf").is_ok());
        assert!(resolve_invoice_path(app_data, "../settings.toml").is_err());
        assert!(resolve_invoice_path(app_data, "/tmp/invoice.pdf").is_err());
    }
}
