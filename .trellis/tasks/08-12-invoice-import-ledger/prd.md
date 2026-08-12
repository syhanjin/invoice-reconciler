# 发票导入与台账

## Goal

为桌面应用增加完整的发票导入、腾讯云 OCR 识别、拆分复核和发票台账能力；同时将应用连接配置迁移到独立的 `settings.toml`，使腾讯云密钥与 Ollama 服务配置可维护且具有明确的本地文件权限保护。

## Requirements

### Settings

* 应用数据目录新增版本化 `settings.toml`，格式版本为 `version = 1`。
* 文件保存 Ollama `base_url`、`model`、`max_concurrency`，以及腾讯云 `secret_id`、`secret_key`、`invoice_ocr_concurrency`。
* 第一版密钥按用户决定明文保存，不引入系统密钥环或加密口令；后续可通过版本字段升级格式。
* Unix 自动设置权限 `0600`；Windows 尝试限制为当前用户。权限收紧失败时继续运行，但设置页持续展示安全警告。
* 原子写入设置文件；SecretKey 不进入日志、错误详情、前端持久状态、事件、SQLite、测试快照。
* 首次运行且 TOML 不存在时，从 SQLite 旧设置迁移 Ollama 配置。之后 TOML 是唯一设置来源，旧数据库列保留兼容但不再写入。
* 设置页可编辑 Ollama 与腾讯云配置。腾讯云只展示是否已配置和脱敏 SecretId；读取设置不回传 SecretKey。
* 不提供会产生费用的“测试连接”；格式仅做本地校验，首次实际导入验证服务开通和余额。

### Import and OCR

* 正式支持 PNG、JPG、JPEG、PDF；明确拒绝 OFD、GIF、BMP、TIFF、WebP、HEIC，不自动转换。
* 单批最多选择 500 个文件。超过 500 个不创建批次并提示用户缩减。
* 先执行纯本地预检：文件总数、有效数、重复文件数、无效/超限数、预计腾讯请求数、PDF 总页数；用户确认后才复制文件、建任务和触发付费请求。
* 预检返回令牌和文件指纹；创建时文件已变化则拒绝，要求重新预检。
* 所有有效输入统一调用腾讯云 OCR `RecognizeGeneralInvoice`，版本 `2018-11-19`，域名 `ocr.tencentcloudapi.com`。
* 默认自动识别全部票种并启用 Other；高级选项允许强制指定单一 Types 值，重试时可修改。
* PDF 开启多页识别，最多处理接口支持的前 30 页。Base64 编码后超过 10 MiB、页数或格式不符合约束的文件作为独立失败任务，不阻止同批其他有效任务。
* OCR 使用独立队列，默认并发 3、可配置 1..=5；全局严格限制不超过 5 QPS。
* 限流与临时网络错误采用有上限的指数退避；不可恢复业务错误直接失败。
* 应用重启把 queued/processing 标为 interrupted，不自动重试，避免重复计费；用户可手动重试。
* 发票任务通过独立 `invoice-task-updated` 事件更新，不复用订单事件。

### Splitting and Review

* 保存完整原始源文件用于审计；每个最终台账记录还要对应一个可独立预览的托管文件。
* 单文件单发票直接建立记录。
* 多页且每页不同发票时，保留为单页 PDF/图片并分别建记录。
* 同页多张混贴票据时，请求腾讯云返回 CutImage，每张切图保存为 PNG 并分别建记录。
* 同一发票跨页时，以可靠发票标识、页序和票种/续页特征高置信合并为一条记录，生成多页 PDF。
* 不能高置信判断的分组进入 `awaiting_review`，不计成功或失败；复核界面允许拆分/合并并确认，确认不重复调用 OCR。
* 每条记录保存标准化字段、票据明细、类型信息、页码/分组关系、腾讯 RequestId 和完整原始响应 JSON。原始响应只读。

### Persistence and Duplicates

* 新增 SeaORM 迁移与发票批次、任务、发票、发票明细、源文件关联表；不修改初始迁移，不引入 SQLx。
* 金额以定点整数加币种存储，避免浮点误差。
* 原始文件 SHA-256 已存在时不调用腾讯云，任务失败为 `duplicate_file`。
* OCR 得到可靠身份（票种 + 代码 + 号码或票种等价可靠字段）且冲突时不建新记录，任务失败为 `duplicate_invoice`；保留已有发票 ID、腾讯 RequestId 和原始响应审计信息。
* 缺少可靠身份时不硬拦截，可标记 `possible_duplicate`。
* 重复票据审计落库后删除本次托管副本；清理失败只产生警告，不改变重复失败状态。
* 台账允许编辑标准化字段。可靠身份修改时重新检查唯一冲突，原始腾讯响应不可编辑。
* 台账删除记录和托管文件，采用与订单删除一致的文件补偿策略；不删除仍被其他记录引用的源文件。

### UI

* 导入任务中心启用“发票”页签，支持批量选文件、预检确认、任务状态、取消、失败详情、重试和待复核入口。
* 新增一级发票台账页面，支持服务端分页 20/50/100、查看、编辑、删除、多选和批量删除。
* 台账搜索发票代码/号码、购销方名称和税号；筛选票种、发票日期范围、疑似重复；按发票日期、总金额、创建时间排序。
* 详情内嵌预览图片和 PDF。PDF 使用 PDF.js 前端渲染，避免依赖系统 PDF 阅读器。
* 页面延续现有 shadcn/Tailwind 设计、导航、表格与任务交互模式。

## Public Interfaces

```text
get_settings() -> PublicAppSettings
save_settings(request: SaveSettingsRequest) -> PublicAppSettings

prepare_invoice_batch(request: PrepareInvoiceBatchRequest) -> InvoiceBatchPreview
create_invoice_batch(request: CreateInvoiceBatchRequest) -> InvoiceBatchDetail
list_invoice_batches(query: InvoiceBatchesQuery) -> InvoiceBatchesPage
get_invoice_batch(request: { id }) -> InvoiceBatchDetail
cancel_invoice_task(request: { id }) -> bool
retry_invoice_task(request: RetryInvoiceTaskRequest) -> InvoiceBatchDetail
confirm_invoice_split(request: ConfirmInvoiceSplitRequest) -> InvoiceBatchDetail

list_invoices(query: InvoicesQuery) -> InvoicesPage
get_invoice(request: { id }) -> InvoiceDetail
update_invoice(request: UpdateInvoiceRequest) -> InvoiceDetail
delete_invoice(request: { id }) -> ()
delete_invoices(request: { ids }) -> BulkDeleteResult
```

## Acceptance Criteria

* [x] 设置文件可原子读写，Unix 权限为 `0600`，失败会在 UI 显示持续警告。
* [x] SecretKey 从不通过读取命令回传前端，日志/错误/数据库中无密钥。
* [x] 旧 Ollama SQLite 设置可一次性迁移，现有 Ollama 分析仍正常工作。
* [x] 最多 500 个支持文件可完成纯本地预检，用户确认前不会调用腾讯 OCR。
* [x] 无效、超限、重复文件分别产生清晰失败原因，其他任务仍可继续。
* [x] 腾讯 TC3-HMAC-SHA256 签名正确，队列并发和 5 QPS 限制可测试。
* [x] OCR 响应可映射常见发票的标准字段和明细，并完整保存原始 JSON。
* [x] 单票、多页多票、同页混贴、明确跨页同票分别生成正确的独立预览文件和记录。
* [x] 模糊跨页分组进入待复核，拆分/合并确认不产生第二次 OCR 调用。
* [x] 重复文件零 OCR 调用；重复发票保留审计数据但不新增台账记录。
* [x] 发票台账的分页、搜索、筛选、排序、查看、编辑、删除、多选批量删除工作正常。
* [x] 图片和 PDF 均可在应用内预览。
* [x] Rust 测试、前端 lint/typecheck/build 通过。

## Definition of Done

* 数据库迁移、后端命令、前端页面和类型完整联通。
* 核心解析、签名、限流、重复检测和文件补偿有自动化测试。
* `cargo fmt`、`cargo test`、`cargo clippy`、Bun lint/typecheck/build 通过。
* 新增的持久化、安全和跨层约定写入 Trellis 规范。
* 只提交本任务产生的文件，不包含现存无关未跟踪 Trellis/bootstrap 文件。

## Technical Approach

* 使用 `reqwest` 直接调用腾讯云 API，并在 Rust 中实现官方 TC3-HMAC-SHA256 签名；不把密钥带入前端。
* 使用 SeaORM entity/migration 保存任务和台账；队列沿用现有订单任务的 Tauri state、后台 worker 与事件模式，但发票状态独立。
* PDF 页面提取/组合使用纯 Rust 库，前端预览使用 `pdfjs-dist`。混贴票据优先使用腾讯 `CutImage`，避免本地坐标裁切差异。
* 标准化字段从 `SingleInvoiceInfos[SubType]` 中按公共字段别名提取；所有未识别字段仍由 raw JSON 保留，保证票种覆盖面。
* 先完成设置与持久化底座，再完成 OCR 队列/解析，最后接入导入任务和台账 UI。

## Decision (ADR-lite)

**Context**: 功能包含敏感配置、付费网络调用、异构票据、多页/混贴文件处理和可追溯台账，必须明确失败与幂等边界。

**Decision**: 采用独立明文 TOML + 本地权限保护、腾讯统一票据 OCR、预检后二次确认、原始响应全量审计、可靠身份硬去重、歧义分组人工复核。

**Consequences**: 明文密钥依赖设备账户和文件权限保护，UI 必须持续暴露权限风险；跨页歧义不会被静默猜测；任务和原始响应会增加本地数据库/文件占用；OCR 调用可能计费，因此自动重试和重启恢复必须保守。

## Out of Scope

* OFD 文件识别。
* 发票与订单自动匹配。
* 报销流、审批状态、发票验真和导出。
* OS Keyring、主密码或第一版设置文件加密。
* 腾讯云临时 SessionToken 凭证。

## Technical Notes

* 腾讯接口：`RecognizeGeneralInvoice`，API version `2018-11-19`，默认限频 5 次/秒，PDF 最多前 30 页。
* 腾讯文档：<https://cloud.tencent.com/document/product/866/90802>
* InvoiceItem 结构：<https://cloud.tencent.com/document/product/866/33527>
* TC3 签名：<https://cloud.tencent.com/document/product/213/30654>
* 当前栈：Tauri v2、Rust、SeaORM/SQLite、React/TypeScript、shadcn/ui、Jotai、Tailwind v4、Bun。
