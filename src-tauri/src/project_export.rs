use std::collections::HashSet;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use lopdf::{Document, Object, dictionary};
use printpdf::{Mm, Op, PdfDocument, PdfPage, PdfSaveOptions, Pt, RawImage, XObjectTransform};
use rust_xlsxwriter::{Color, Format, FormatBorder, Workbook};
use tauri::State;
use uuid::Uuid;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::AppState;
use crate::error::{AppError, AppResult, CommandResult};
use crate::models::{ExportProjectRequest, ExportProjectResult};
use crate::projects::{ProjectExportInvoice, ProjectExportSnapshot};

const EXCEL_NAME: &str = "发票明细.xlsx";
const COMBINED_PDF_NAME: &str = "组合材料.pdf";

pub(crate) async fn export_project_package(
    state: &AppState,
    request: ExportProjectRequest,
) -> AppResult<ExportProjectResult> {
    let snapshot = state
        .database
        .project_export_snapshot(&request.id, &state.app_data_dir)
        .await?;
    let actual_amount_fen = validate_export_snapshot(&snapshot)?;
    let output_directory = PathBuf::from(request.output_directory);
    if !output_directory.is_dir() {
        return Err(AppError::InvalidInput("导出目录不存在或不是文件夹".into()));
    }
    let file_name = export_file_name(&snapshot.name, actual_amount_fen);
    let target = output_directory.join(file_name);
    if target.exists() && !request.overwrite {
        return Err(AppError::DestinationExists(
            target.to_string_lossy().into_owned(),
        ));
    }
    if target.exists() && !target.is_file() {
        return Err(AppError::InvalidInput("同名导出目标不是普通文件".into()));
    }

    generate_and_install_archive(snapshot.clone(), target.clone(), request.overwrite).await?;

    Ok(ExportProjectResult {
        path: target.to_string_lossy().into_owned(),
        invoice_count: snapshot.invoices.len() as i64,
        actual_amount_fen,
    })
}

async fn generate_and_install_archive(
    snapshot: ProjectExportSnapshot,
    target: PathBuf,
    overwrite: bool,
) -> AppResult<()> {
    let output_directory = target
        .parent()
        .ok_or_else(|| AppError::InvalidInput("导出目标缺少父目录".into()))?;
    let token = Uuid::now_v7();
    let temporary = output_directory.join(format!(".{token}.project-export.tmp"));
    let backup = output_directory.join(format!(".{token}.project-export.bak"));
    let snapshot_for_generation = snapshot.clone();
    let temporary_for_generation = temporary.clone();
    let generation = match tokio::task::spawn_blocking(move || {
        generate_archive(&snapshot_for_generation, &temporary_for_generation)
    })
    .await
    {
        Ok(result) => result,
        Err(error) => {
            let _ = tokio::fs::remove_file(&temporary).await;
            return Err(AppError::Internal(format!("导出任务异常结束：{error}")));
        }
    };
    if let Err(error) = generation {
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err(error);
    }

    if target.exists() && !overwrite {
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err(AppError::DestinationExists(
            target.to_string_lossy().into_owned(),
        ));
    }
    if target.exists() && !target.is_file() {
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err(AppError::InvalidInput("同名导出目标不是普通文件".into()));
    }

    if target.exists() {
        if let Err(error) = tokio::fs::rename(&target, &backup).await {
            let _ = tokio::fs::remove_file(&temporary).await;
            return Err(AppError::File(format!("无法备份现有导出文件：{error}")));
        }
        if let Err(error) = tokio::fs::rename(&temporary, &target).await {
            let restore = tokio::fs::rename(&backup, &target).await;
            let _ = tokio::fs::remove_file(&temporary).await;
            return Err(AppError::File(match restore {
                Ok(()) => format!("无法替换现有导出文件，旧文件已恢复：{error}"),
                Err(restore_error) => {
                    format!("无法替换导出文件且旧文件恢复失败：{error}；{restore_error}")
                }
            }));
        }
        let _ = tokio::fs::remove_file(&backup).await;
    } else if let Err(error) = tokio::fs::rename(&temporary, &target).await {
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err(AppError::File(format!("无法保存导出文件：{error}")));
    }

    Ok(())
}

fn validate_export_snapshot(snapshot: &ProjectExportSnapshot) -> AppResult<i64> {
    if snapshot.invoices.is_empty() {
        return Err(AppError::InvalidInput("项目暂无发票，不能导出".into()));
    }
    let mut total = 0_i64;
    for invoice in &snapshot.invoices {
        if let Some(reason) = &invoice.member.blocking_reason {
            let label = invoice
                .member
                .invoice_number
                .as_deref()
                .unwrap_or(&invoice.member.id);
            return Err(AppError::InvalidInput(format!("发票 {label}：{reason}")));
        }
        let actual = invoice.member.actual_amount_fen.ok_or_else(|| {
            AppError::InvalidInput(format!("发票 {} 实际金额无法计算", invoice.member.id))
        })?;
        total = total
            .checked_add(actual)
            .ok_or_else(|| AppError::InvalidInput("项目实际金额超出支持范围".into()))?;
    }
    Ok(total)
}

fn generate_archive(snapshot: &ProjectExportSnapshot, destination: &Path) -> AppResult<()> {
    let excel = generate_excel(snapshot)?;
    let combined_pdf = generate_combined_pdf(snapshot)?;
    let file = File::create(destination)?;
    let mut archive = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    write_bytes(&mut archive, EXCEL_NAME, &excel, options)?;
    write_bytes(&mut archive, COMBINED_PDF_NAME, &combined_pdf, options)?;

    for (index, invoice) in snapshot.invoices.iter().enumerate() {
        let folder = material_folder_name(index, invoice);
        let invoice_extension = required_extension(&invoice.preview_path, "发票独立材料")?;
        let invoice_label = invoice
            .member
            .invoice_number
            .as_deref()
            .unwrap_or(&invoice.member.id);
        let invoice_name = format!(
            "{folder}/发票_{}.{}",
            safe_component(invoice_label, "发票", 80),
            invoice_extension
        );
        write_file(&mut archive, &invoice_name, &invoice.preview_path, options)?;

        for (order_index, order) in invoice.orders.iter().enumerate() {
            let extension = required_extension(&order.image_path, "订单原文件")?;
            let label = order.external_order_number.as_deref().unwrap_or(&order.id);
            let order_name = format!(
                "{folder}/订单_{:02}_{}.{}",
                order_index + 1,
                safe_component(label, "订单", 80),
                extension
            );
            write_file(&mut archive, &order_name, &order.image_path, options)?;
        }
    }
    archive
        .finish()
        .map_err(|error| AppError::File(format!("无法完成 ZIP 文件：{error}")))?;
    Ok(())
}

fn generate_excel(snapshot: &ProjectExportSnapshot) -> AppResult<Vec<u8>> {
    const HEADERS: [&str; 16] = [
        "序号",
        "发票类型",
        "发票代码",
        "发票号码",
        "开票日期",
        "购买方名称",
        "购买方税号",
        "销售方名称",
        "销售方税号",
        "不含税金额",
        "税额",
        "发票金额",
        "订单金额",
        "实际金额",
        "币种",
        "确认订单号",
    ];
    const WIDTHS: [f64; 16] = [
        8.0, 18.0, 18.0, 22.0, 14.0, 26.0, 22.0, 26.0, 22.0, 14.0, 12.0, 14.0, 14.0, 14.0, 10.0,
        30.0,
    ];
    let header_format = Format::new()
        .set_bold()
        .set_background_color(Color::RGB(0xE4E4E7))
        .set_border(FormatBorder::Thin);
    let money_format = Format::new().set_num_format("0.00");
    let mut workbook = Workbook::new();
    let worksheet = workbook.add_worksheet();
    worksheet.set_name("发票明细").map_err(xlsx_error)?;
    worksheet.set_freeze_panes(1, 0).map_err(xlsx_error)?;
    for (column, header) in HEADERS.iter().enumerate() {
        worksheet
            .write_string_with_format(0, column as u16, *header, &header_format)
            .map_err(xlsx_error)?;
        worksheet
            .set_column_width(column as u16, WIDTHS[column])
            .map_err(xlsx_error)?;
    }

    for (index, invoice) in snapshot.invoices.iter().enumerate() {
        let row = (index + 1) as u32;
        let member = &invoice.member;
        worksheet
            .write_number(row, 0, (index + 1) as f64)
            .map_err(xlsx_error)?;
        write_optional_text(worksheet, row, 1, Some(&member.type_description))?;
        write_optional_text(worksheet, row, 2, member.invoice_code.as_deref())?;
        write_optional_text(worksheet, row, 3, member.invoice_number.as_deref())?;
        write_optional_text(worksheet, row, 4, member.invoice_date.as_deref())?;
        write_optional_text(worksheet, row, 5, member.buyer_name.as_deref())?;
        write_optional_text(worksheet, row, 6, member.buyer_tax_id.as_deref())?;
        write_optional_text(worksheet, row, 7, member.seller_name.as_deref())?;
        write_optional_text(worksheet, row, 8, member.seller_tax_id.as_deref())?;
        write_optional_money(worksheet, row, 9, member.amount_fen, &money_format)?;
        write_optional_money(worksheet, row, 10, member.tax_fen, &money_format)?;
        write_optional_money(worksheet, row, 11, member.invoice_amount_fen, &money_format)?;
        write_optional_money(worksheet, row, 12, member.order_amount_fen, &money_format)?;
        write_optional_money(worksheet, row, 13, member.actual_amount_fen, &money_format)?;
        write_optional_text(worksheet, row, 14, Some(&member.currency))?;
        let order_numbers = invoice
            .orders
            .iter()
            .map(|order| order.external_order_number.as_deref().unwrap_or(&order.id))
            .collect::<Vec<_>>()
            .join("、");
        write_optional_text(worksheet, row, 15, Some(&order_numbers))?;
    }
    worksheet
        .autofilter(0, 0, snapshot.invoices.len() as u32, 15)
        .map_err(xlsx_error)?;
    workbook.save_to_buffer().map_err(xlsx_error)
}

fn generate_combined_pdf(snapshot: &ProjectExportSnapshot) -> AppResult<Vec<u8>> {
    let mut documents = Vec::new();
    let mut warnings = Vec::new();
    for invoice in &snapshot.invoices {
        for order in &invoice.orders {
            let bytes = std::fs::read(&order.image_path)?;
            let document = image_document(&bytes, "订单", &mut warnings)?;
            let bytes = document.save(&PdfSaveOptions::default(), &mut warnings);
            documents.push(
                Document::load_mem(&bytes)
                    .map_err(|error| AppError::File(format!("订单单页 PDF 无法解析：{error}")))?,
            );
        }
        let bytes = std::fs::read(&invoice.preview_path)?;
        if invoice
            .preview_path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("pdf"))
        {
            documents.push(
                Document::load_mem(&bytes)
                    .map_err(|error| AppError::File(format!("发票 PDF 无法解析：{error}")))?,
            );
        } else {
            let document = image_document(&bytes, "发票", &mut warnings)?;
            let bytes = document.save(&PdfSaveOptions::default(), &mut warnings);
            documents.push(
                Document::load_mem(&bytes)
                    .map_err(|error| AppError::File(format!("发票单页 PDF 无法解析：{error}")))?,
            );
        }
    }
    merge_pdf_documents(documents)
}

fn merge_pdf_documents(mut documents: Vec<Document>) -> AppResult<Vec<u8>> {
    if documents.is_empty() {
        return Err(AppError::InvalidInput("项目没有可合并的材料页".into()));
    }

    let mut combined = Document::with_version("1.7");
    let mut next_object_id = 1_u32;
    let mut pages = Vec::new();
    for document in &mut documents {
        let source_pages = document.get_pages().into_values().collect::<Vec<_>>();
        if source_pages.is_empty() {
            return Err(AppError::File("材料 PDF 不包含页面".into()));
        }
        for page_id in source_pages {
            materialize_inherited_page_attributes(document, page_id)?;
        }

        document.renumber_objects_with(next_object_id);
        next_object_id = document.max_id.saturating_add(1);
        for page_id in document.get_pages().into_values() {
            let page = document
                .get_object(page_id)
                .map_err(|error| AppError::File(format!("无法读取材料 PDF 页面：{error}")))?
                .clone();
            pages.push((page_id, page));
        }
        for (object_id, object) in std::mem::take(&mut document.objects) {
            if !matches!(
                object.type_name().unwrap_or(b""),
                b"Catalog" | b"Pages" | b"Page" | b"Outlines" | b"Outline"
            ) {
                combined.objects.insert(object_id, object);
            }
        }
    }

    let pages_id = (next_object_id, 0);
    let catalog_id = (next_object_id.saturating_add(1), 0);
    let mut page_ids = Vec::with_capacity(pages.len());
    for (page_id, page) in pages {
        let mut dictionary = page
            .as_dict()
            .map_err(|error| AppError::File(format!("材料 PDF 页面结构无效：{error}")))?
            .clone();
        dictionary.set("Parent", pages_id);
        combined
            .objects
            .insert(page_id, Object::Dictionary(dictionary));
        page_ids.push(page_id);
    }
    combined.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => page_ids.iter().copied().map(Object::Reference).collect::<Vec<_>>(),
            "Count" => page_ids.len() as i64,
        }),
    );
    combined.objects.insert(
        catalog_id,
        Object::Dictionary(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        }),
    );
    combined.trailer.set("Root", catalog_id);
    combined.max_id = catalog_id.0;
    combined.renumber_objects();
    combined.compress();

    let mut bytes = Vec::new();
    combined
        .save_to(&mut bytes)
        .map_err(|error| AppError::File(format!("无法生成组合 PDF：{error}")))?;
    Ok(bytes)
}

fn materialize_inherited_page_attributes(
    document: &mut Document,
    page_id: lopdf::ObjectId,
) -> AppResult<()> {
    const INHERITED_KEYS: [&[u8]; 4] = [b"Resources", b"MediaBox", b"CropBox", b"Rotate"];
    for key in INHERITED_KEYS {
        let page = document
            .get_dictionary(page_id)
            .map_err(|error| AppError::File(format!("材料 PDF 页面结构无效：{error}")))?;
        if page.get(key).is_ok() {
            continue;
        }
        let mut parent = page
            .get(b"Parent")
            .ok()
            .and_then(|value| value.as_reference().ok());
        let mut visited = HashSet::new();
        let mut inherited = None;
        while let Some(parent_id) = parent {
            if !visited.insert(parent_id) {
                return Err(AppError::File("材料 PDF 页面树存在循环引用".into()));
            }
            let dictionary = document
                .get_dictionary(parent_id)
                .map_err(|error| AppError::File(format!("材料 PDF 页面树无效：{error}")))?;
            if let Ok(value) = dictionary.get(key) {
                inherited = Some(value.clone());
                break;
            }
            parent = dictionary
                .get(b"Parent")
                .ok()
                .and_then(|value| value.as_reference().ok());
        }
        if let Some(value) = inherited {
            document
                .get_dictionary_mut(page_id)
                .map_err(|error| AppError::File(format!("材料 PDF 页面结构无效：{error}")))?
                .set(key, value);
        }
    }
    Ok(())
}

fn image_document(
    bytes: &[u8],
    title: &str,
    warnings: &mut Vec<printpdf::PdfWarnMsg>,
) -> AppResult<PdfDocument> {
    let image = RawImage::decode_from_bytes(bytes, warnings)
        .map_err(|error| AppError::File(format!("{title}图片无法解析：{error}")))?;
    let landscape = image.width > image.height;
    let (page_width_mm, page_height_mm) = if landscape {
        (297.0_f32, 210.0_f32)
    } else {
        (210.0_f32, 297.0_f32)
    };
    let page_width_pt = Pt::from(Mm(page_width_mm)).0;
    let page_height_pt = Pt::from(Mm(page_height_mm)).0;
    let margin = 24.0_f32;
    let dpi = 300.0_f32;
    let natural_width = image.width as f32 * 72.0 / dpi;
    let natural_height = image.height as f32 * 72.0 / dpi;
    let scale = ((page_width_pt - margin * 2.0) / natural_width)
        .min((page_height_pt - margin * 2.0) / natural_height);
    let rendered_width = natural_width * scale;
    let rendered_height = natural_height * scale;
    let mut document = PdfDocument::new(title);
    let image_id = document.add_image(&image);
    let page = PdfPage::new(
        Mm(page_width_mm),
        Mm(page_height_mm),
        vec![Op::UseXobject {
            id: image_id,
            transform: XObjectTransform {
                translate_x: Some(Pt((page_width_pt - rendered_width) / 2.0)),
                translate_y: Some(Pt((page_height_pt - rendered_height) / 2.0)),
                scale_x: Some(scale),
                scale_y: Some(scale),
                dpi: Some(dpi),
                ..Default::default()
            },
        }],
    );
    document.with_pages(vec![page]);
    Ok(document)
}

fn material_folder_name(index: usize, invoice: &ProjectExportInvoice) -> String {
    let product_name = invoice
        .orders
        .iter()
        .flat_map(|order| order.product_names.iter())
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join("、");
    let fallback_orders = invoice
        .orders
        .iter()
        .map(|order| order.external_order_number.as_deref().unwrap_or(&order.id))
        .collect::<Vec<_>>()
        .join("、");
    let label = if product_name.is_empty() {
        fallback_orders
    } else {
        product_name
    };
    format!(
        "{:03}_{}_{}",
        index + 1,
        safe_component(&label, "订单", 80),
        format_fen(invoice.member.actual_amount_fen.unwrap_or_default())
    )
}

fn export_file_name(project_name: &str, actual_amount_fen: i64) -> String {
    format!(
        "{}_{}.zip",
        safe_component(project_name, "项目", 100),
        format_fen(actual_amount_fen)
    )
}

fn safe_component(value: &str, fallback: &str, max_chars: usize) -> String {
    let mut result = value
        .chars()
        .map(|character| {
            if character.is_control()
                || matches!(
                    character,
                    '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
                )
            {
                '_'
            } else {
                character
            }
        })
        .take(max_chars)
        .collect::<String>();
    result = result.trim().trim_end_matches(['.', ' ']).to_owned();
    if result.is_empty()
        || matches!(
            result.to_ascii_uppercase().as_str(),
            "CON"
                | "PRN"
                | "AUX"
                | "NUL"
                | "COM1"
                | "COM2"
                | "COM3"
                | "COM4"
                | "COM5"
                | "COM6"
                | "COM7"
                | "COM8"
                | "COM9"
                | "LPT1"
                | "LPT2"
                | "LPT3"
                | "LPT4"
                | "LPT5"
                | "LPT6"
                | "LPT7"
                | "LPT8"
                | "LPT9"
        )
    {
        fallback.to_owned()
    } else {
        result
    }
}

fn format_fen(value: i64) -> String {
    format!("{}.{:02}", value / 100, value % 100)
}

fn required_extension(path: &Path, label: &str) -> AppResult<String> {
    path.extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::File(format!("{label}缺少文件扩展名")))
}

fn write_bytes(
    archive: &mut ZipWriter<File>,
    name: &str,
    bytes: &[u8],
    options: SimpleFileOptions,
) -> AppResult<()> {
    archive
        .start_file(name, options)
        .map_err(|error| AppError::File(format!("无法创建 ZIP 条目 {name}：{error}")))?;
    archive.write_all(bytes)?;
    Ok(())
}

fn write_file(
    archive: &mut ZipWriter<File>,
    name: &str,
    path: &Path,
    options: SimpleFileOptions,
) -> AppResult<()> {
    archive
        .start_file(name, options)
        .map_err(|error| AppError::File(format!("无法创建 ZIP 条目 {name}：{error}")))?;
    let mut source = File::open(path)?;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = source.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        archive.write_all(&buffer[..read])?;
    }
    Ok(())
}

fn write_optional_text(
    worksheet: &mut rust_xlsxwriter::Worksheet,
    row: u32,
    column: u16,
    value: Option<&str>,
) -> AppResult<()> {
    if let Some(value) = value.filter(|value| !value.is_empty()) {
        worksheet
            .write_string(row, column, value)
            .map_err(xlsx_error)?;
    }
    Ok(())
}

fn write_optional_money(
    worksheet: &mut rust_xlsxwriter::Worksheet,
    row: u32,
    column: u16,
    value: Option<i64>,
    format: &Format,
) -> AppResult<()> {
    if let Some(value) = value {
        worksheet
            .write_number_with_format(row, column, value as f64 / 100.0, format)
            .map_err(xlsx_error)?;
    }
    Ok(())
}

fn xlsx_error(error: rust_xlsxwriter::XlsxError) -> AppError {
    AppError::File(format!("无法生成 Excel：{error}"))
}

#[tauri::command]
pub async fn export_project(
    state: State<'_, AppState>,
    request: ExportProjectRequest,
) -> CommandResult<ExportProjectResult> {
    export_project_package(&state, request)
        .await
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ProjectInvoiceMember, ProjectOrderSummary};
    use crate::projects::{ProjectExportInvoice, ProjectExportOrder, ProjectExportSnapshot};
    use std::io::Cursor;

    fn write_pdf(path: &Path, page_count: usize) {
        let mut document = Document::with_version("1.5");
        let pages_id = document.new_object_id();
        let contents_id = document.add_object(lopdf::Stream::new(dictionary! {}, Vec::new()));
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

    #[test]
    fn safe_names_replace_cross_platform_reserved_characters() {
        assert_eq!(safe_component("a/b:c*", "fallback", 80), "a_b_c_");
        assert_eq!(safe_component("CON", "fallback", 80), "fallback");
        assert_eq!(safe_component(" ... ", "fallback", 80), "fallback");
        assert_eq!(format_fen(12345), "123.45");
        assert_eq!(export_file_name("差旅/项目", 12345), "差旅_项目_123.45.zip");
    }

    #[tokio::test]
    async fn archive_contains_ordered_materials_and_combined_pages() {
        let directory = tempfile::tempdir().unwrap();
        let order_a = directory.path().join("order-a.png");
        let order_b = directory.path().join("order-b.png");
        let invoice = directory.path().join("invoice.pdf");
        image::RgbImage::from_pixel(320, 180, image::Rgb([220, 20, 60]))
            .save(&order_a)
            .unwrap();
        image::RgbImage::from_pixel(180, 320, image::Rgb([30, 144, 255]))
            .save(&order_b)
            .unwrap();
        write_pdf(&invoice, 2);
        let public_orders = vec![
            ProjectOrderSummary {
                id: "order-a".into(),
                external_order_number: Some("A/1".into()),
                paid_amount_fen: Some(600),
                product_names: vec!["办公/用品".into()],
            },
            ProjectOrderSummary {
                id: "order-b".into(),
                external_order_number: Some("B-2".into()),
                paid_amount_fen: Some(700),
                product_names: vec!["墨盒".into()],
            },
        ];
        let snapshot = ProjectExportSnapshot {
            id: "project".into(),
            name: "测试项目".into(),
            created_at: "now".into(),
            updated_at: "now".into(),
            invoices: vec![ProjectExportInvoice {
                member: ProjectInvoiceMember {
                    id: "invoice-a".into(),
                    position: 0,
                    type_description: "增值税发票".into(),
                    invoice_code: Some("CODE".into()),
                    invoice_number: Some("INV/1".into()),
                    invoice_date: Some("2026-08-13".into()),
                    buyer_name: Some("购买方".into()),
                    buyer_tax_id: Some("BUYER".into()),
                    seller_name: Some("销售方".into()),
                    seller_tax_id: Some("SELLER".into()),
                    amount_fen: Some(900),
                    tax_fen: Some(100),
                    invoice_amount_fen: Some(1000),
                    order_amount_fen: Some(1300),
                    actual_amount_fen: Some(1000),
                    currency: "CNY".into(),
                    confirmed: true,
                    file_available: true,
                    blocking_reason: None,
                    orders: public_orders,
                },
                preview_path: invoice,
                orders: vec![
                    ProjectExportOrder {
                        id: "order-a".into(),
                        external_order_number: Some("A/1".into()),
                        product_names: vec!["办公/用品".into()],
                        image_path: order_a,
                    },
                    ProjectExportOrder {
                        id: "order-b".into(),
                        external_order_number: Some("B-2".into()),
                        product_names: vec!["墨盒".into()],
                        image_path: order_b,
                    },
                ],
            }],
        };
        let destination = directory.path().join("package.zip");
        std::fs::write(&destination, b"previous archive").unwrap();
        assert!(matches!(
            generate_and_install_archive(snapshot.clone(), destination.clone(), false).await,
            Err(AppError::DestinationExists(_))
        ));
        assert_eq!(std::fs::read(&destination).unwrap(), b"previous archive");
        generate_and_install_archive(snapshot, destination.clone(), true)
            .await
            .unwrap();
        let file = File::open(destination).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        let mut excel = Vec::new();
        archive
            .by_name(EXCEL_NAME)
            .unwrap()
            .read_to_end(&mut excel)
            .unwrap();
        let mut workbook = zip::ZipArchive::new(Cursor::new(excel)).unwrap();
        let mut sheet = String::new();
        workbook
            .by_name("xl/worksheets/sheet1.xml")
            .unwrap()
            .read_to_string(&mut sheet)
            .unwrap();
        let mut shared_strings = String::new();
        workbook
            .by_name("xl/sharedStrings.xml")
            .unwrap()
            .read_to_string(&mut shared_strings)
            .unwrap();
        for header in [
            "序号",
            "发票类型",
            "发票代码",
            "发票号码",
            "开票日期",
            "购买方名称",
            "购买方税号",
            "销售方名称",
            "销售方税号",
            "不含税金额",
            "税额",
            "发票金额",
            "订单金额",
            "实际金额",
            "币种",
            "确认订单号",
        ] {
            assert!(
                shared_strings.contains(header),
                "missing XLSX header {header}"
            );
        }
        assert_xlsx_number(&sheet, "L2", "10");
        assert_xlsx_number(&sheet, "M2", "13");
        assert_xlsx_number(&sheet, "N2", "10");
        let mut pdf = Vec::new();
        archive
            .by_name(COMBINED_PDF_NAME)
            .unwrap()
            .read_to_end(&mut pdf)
            .unwrap();
        let combined = lopdf::Document::load_mem(&pdf).unwrap();
        assert_eq!(combined.get_pages().len(), 4);
        let first_order = page_size(&combined, 1);
        let second_order = page_size(&combined, 2);
        let first_invoice = page_size(&combined, 3);
        assert!(first_order.0 > first_order.1);
        assert!(second_order.0 < second_order.1);
        assert_eq!(first_invoice, (595.0, 842.0));
        let folder = "001_办公_用品、墨盒_10.00";
        let mut invoice_pdf = Vec::new();
        archive
            .by_name(&format!("{folder}/发票_INV_1.pdf"))
            .unwrap()
            .read_to_end(&mut invoice_pdf)
            .unwrap();
        assert_eq!(
            lopdf::Document::load_mem(&invoice_pdf)
                .unwrap()
                .get_pages()
                .len(),
            2
        );
        assert!(
            archive
                .by_name(&format!("{folder}/订单_01_A_1.png"))
                .is_ok()
        );
        assert!(
            archive
                .by_name(&format!("{folder}/订单_02_B-2.png"))
                .is_ok()
        );
    }

    #[tokio::test]
    async fn failed_generation_preserves_an_existing_archive() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("existing.zip");
        std::fs::write(&target, b"previous archive").unwrap();
        let snapshot = ProjectExportSnapshot {
            id: "project".into(),
            name: "失败项目".into(),
            created_at: "now".into(),
            updated_at: "now".into(),
            invoices: vec![ProjectExportInvoice {
                member: ProjectInvoiceMember {
                    id: "invoice".into(),
                    position: 0,
                    type_description: "增值税发票".into(),
                    invoice_code: None,
                    invoice_number: None,
                    invoice_date: None,
                    buyer_name: None,
                    buyer_tax_id: None,
                    seller_name: None,
                    seller_tax_id: None,
                    amount_fen: None,
                    tax_fen: None,
                    invoice_amount_fen: Some(100),
                    order_amount_fen: Some(100),
                    actual_amount_fen: Some(100),
                    currency: "CNY".into(),
                    confirmed: true,
                    file_available: true,
                    blocking_reason: None,
                    orders: Vec::new(),
                },
                preview_path: directory.path().join("missing.pdf"),
                orders: Vec::new(),
            }],
        };

        assert!(
            generate_and_install_archive(snapshot, target.clone(), true)
                .await
                .is_err()
        );
        assert_eq!(std::fs::read(&target).unwrap(), b"previous archive");
        assert!(std::fs::read_dir(directory.path()).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains("project-export")
        }));
    }

    fn assert_xlsx_number(sheet: &str, reference: &str, value: &str) {
        let cell_start = sheet.find(&format!("r=\"{reference}\"")).unwrap();
        let cell = &sheet[cell_start..sheet.len().min(cell_start + 160)];
        assert!(
            cell.contains(&format!("<v>{value}</v>")),
            "cell {reference} is not numeric {value}: {cell}"
        );
    }

    fn page_size(document: &Document, page: u32) -> (f64, f64) {
        let page_id = document.get_pages()[&page];
        let media_box = document
            .get_dictionary(page_id)
            .unwrap()
            .get(b"MediaBox")
            .unwrap()
            .as_array()
            .unwrap();
        let number = |value: &Object| match value {
            Object::Integer(value) => *value as f64,
            Object::Real(value) => f64::from(*value),
            other => panic!("unexpected MediaBox value {other:?}"),
        };
        (
            number(&media_box[2]) - number(&media_box[0]),
            number(&media_box[3]) - number(&media_box[1]),
        )
    }
}
