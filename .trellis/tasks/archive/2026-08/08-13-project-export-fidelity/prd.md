# 修复项目材料导出清晰度

## Goal

修复项目材料导出时订单图片和发票材料出现严重清晰度下降的问题。导出应遵循“非必要不转图片，非必要图片不压缩”，优先保留现有受管文件的原始格式与内容。

## Requirements

- ZIP 内订单独立材料保持受管原图字节，不转格式、不缩放、不进行有损压缩。
- 订单进入 `组合材料.pdf` 时保留完整原始像素，不降采样，并使用无损 PDF 图像流封装；允许由此导致 PDF 和 ZIP 显著增大。
- 发票优先使用受管原始源文件。仅当原始文件包含多张发票、只需其中部分 PDF 页面，或同页多票必须裁剪时，使用独立预览。
- PDF 发票直接按 PDF 页面合并，不栅格化或重绘。部分 PDF 页面仍以 PDF 页面抽取结果合并。
- 只有同页多票必须拆分时才允许使用腾讯 OCR 切图或本地无损 PNG 裁剪结果。
- 独立发票材料与组合 PDF 使用同一材料选择结果，不能泄露同一源文件中的其他发票。
- 保持项目顺序，以及每组订单页在前、发票页在后的顺序。
- 不改变公开 Tauri API、前端交互或数据库结构。

## Acceptance Criteria

- [x] 高分辨率订单 JPEG/PNG 的独立导出文件与受管原图逐字节一致。
- [x] 组合 PDF 的订单图像保留原像素宽高，不出现 resize，且不使用 JPEG 等有损重编码。
- [x] 单张图片只对应一张发票时使用原始源文件。
- [x] 完整单票 PDF 使用原始 PDF；部分页发票使用 PDF 页面抽取结果，不转图片。
- [x] 同页多票继续使用独立裁剪材料，不将其他发票带入导出。
- [x] 多票源文件在删除、恢复或项目只包含部分发票时不会被误当作独立原件。
- [x] 既有 ZIP/XLSX/PDF 结构、顺序、失败清理和覆盖恢复测试继续通过。
- [x] Rust 格式、Clippy（warnings denied）和完整 Rust 测试通过。

## Technical Notes

- `printpdf::PdfSaveOptions::default()` 启用图片自动优化，默认对彩色图片使用约 85% 质量 JPEG 编码，并设置 2 MB 目标，这是当前清晰度下降的直接原因。
- 发票任务已持久化 `source_path`、`media_type`，发票及来源关联已记录 `page_start`/`page_end`，可用于证明何时原始源文件与独立发票等价。
- ZIP Deflate 和 PDF Flate 属于无损压缩；“不压缩”的清晰度要求落地为不降采样、不改变像素、不做有损重编码。订单和发票独立图片条目使用 ZIP Stored。
- 相关规范：`.trellis/spec/backend/project-export-contract.md`、`.trellis/spec/backend/invoice-import-contract.md`、`.trellis/spec/backend/quality-guidelines.md`。

## Out of Scope

- 不取消 `组合材料.pdf`。
- 不修改导入阶段的 OCR、裁剪或源文件管理流程。
- 不增加导出质量设置或前端选项。
