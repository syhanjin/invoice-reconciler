export type NavigationView = "orders" | "invoices" | "importTasks" | "settings"

export interface AppSettings {
  ollamaBaseUrl: string
  ollamaModel: string
  maxConcurrency: number
  tencentSecretIdMasked: string | null
  tencentCredentialsConfigured: boolean
  invoiceOcrConcurrency: number
  settingsPath: string
  permissionWarning: string | null
}

export interface SaveSettingsRequest {
  ollamaBaseUrl: string
  ollamaModel: string
  maxConcurrency: number
  tencentSecretId: string | null
  tencentSecretKey: string | null
  clearTencentCredentials: boolean
  invoiceOcrConcurrency: number
}

export type TaskStatus =
  | "queued"
  | "processing"
  | "succeeded"
  | "failed"
  | "canceled"
  | "skipped"
  | "interrupted"

export interface ImportTask {
  id: string
  batchId: string
  originalFileName: string
  managedImagePath: string | null
  imageSha256: string
  status: TaskStatus
  errorCode: string | null
  errorMessage: string | null
  warningMessage: string | null
  retryCount: number
  createdAt: string
  startedAt: string | null
  completedAt: string | null
}

export interface BatchCounts {
  total: number
  queued: number
  processing: number
  succeeded: number
  failed: number
  canceled: number
  skipped: number
  interrupted: number
}

export interface BatchDetail {
  id: string
  platform: "taobao"
  status: "queued" | "processing" | "completed" | "completed_with_errors"
  createdAt: string
  completedAt: string | null
  counts: BatchCounts
  tasks: ImportTask[]
}

export interface ImportBatchSummary {
  id: string
  platform: "taobao"
  status: BatchDetail["status"]
  createdAt: string
  completedAt: string | null
  counts: BatchCounts
}

export interface ImportBatchesPage {
  items: ImportBatchSummary[]
  total: number
  page: number
  pageSize: number
}

export interface ImportBatchesQuery {
  page?: number
  pageSize?: number
}

export interface BootstrapState {
  settings: AppSettings
  latestBatch: BatchDetail | null
  latestInvoiceBatch: InvoiceBatchDetail | null
}

export interface TaskUpdateEvent {
  task: ImportTask
  batch: BatchDetail
}

export interface OllamaStatus {
  reachable: boolean
  modelAvailable: boolean
  version: string | null
  message: string
}

export interface OrderListItem {
  id: string
  platform: "taobao"
  externalOrderNumber: string | null
  paidAmountFen: number | null
  orderedAt: string | null
  shopName: string | null
  possibleDuplicate: boolean
  createdAt: string
  productSummary: string
  productCount: number
}

export interface OrdersPage {
  items: OrderListItem[]
  total: number
  page: number
  pageSize: number
}

export interface DeleteOrderResult {
  id: string
  deleted: boolean
  errorCode: string | null
  errorMessage: string | null
}

export interface DeleteOrdersResult {
  results: DeleteOrderResult[]
}

export type OrderSortBy = "created_at" | "ordered_at" | "paid_amount" | "shop_name"
export type SortDirection = "asc" | "desc"

export interface OrdersQuery {
  page?: number
  pageSize?: number
  search?: string
  duplicateOnly?: boolean
  sortBy?: OrderSortBy
  sortDirection?: SortDirection
}

export type OrderTableDensity = "comfortable" | "compact"

export type OrderTableColumnId =
  | "externalOrderNumber"
  | "productSummary"
  | "shopName"
  | "paidAmountFen"
  | "orderedAt"
  | "createdAt"
  | "platform"

export interface OrderTablePreferences {
  pageSize: number
  sortBy: OrderSortBy
  sortDirection: SortDirection
  columnVisibility: Partial<Record<OrderTableColumnId, boolean>>
  columnSizing: Partial<Record<OrderTableColumnId, number>>
  density: OrderTableDensity
}

export interface OrderDetail {
  id: string
  platform: "taobao"
  externalOrderNumber: string | null
  productNames: string[]
  paidAmountFen: number | null
  orderedAt: string | null
  shopName: string | null
  rawOcrJson: string
  possibleDuplicate: boolean
  createdAt: string
  updatedAt: string
  originalFileName: string
  imagePath: string
  warningMessage: string | null
}

export interface UpdateOrderRequest {
  id: string
  externalOrderNumber: string | null
  productNames: string[]
  paidAmountFen: number | null
  orderedAt: string | null
  shopName: string | null
}

export interface CommandError {
  code?: string
  message?: string
}

export type InvoiceTaskStatus =
  | "queued"
  | "processing"
  | "awaiting_review"
  | "succeeded"
  | "failed"
  | "canceled"
  | "interrupted"

export interface InvoicePreflightFile {
  path: string
  originalFileName: string
  mediaType: string
  sizeBytes: number
  pdfPages: number
  status: "valid" | "duplicate" | "invalid"
  message: string | null
}

export interface InvoiceBatchPreview {
  preflightToken: string
  selectedCount: number
  validCount: number
  duplicateCount: number
  invalidCount: number
  predictedRequests: number
  totalPdfPages: number
  forcedType: number | null
  files: InvoicePreflightFile[]
}

export interface InvoiceTask {
  id: string
  batchId: string
  originalFileName: string
  sourcePath: string | null
  sourceSha256: string
  mediaType: string
  forcedType: number | null
  status: InvoiceTaskStatus
  errorCode: string | null
  errorMessage: string | null
  warningMessage: string | null
  retryCount: number
  requestId: string | null
  existingInvoiceId: string | null
  reviewItems: InvoiceReviewItem[]
  createdAt: string
  startedAt: string | null
  completedAt: string | null
}

export interface InvoiceReviewItem {
  index: number
  page: number
  subtype: string
  subtypeDescription: string
  invoiceNumber: string | null
}

export interface InvoiceBatchCounts {
  total: number
  queued: number
  processing: number
  awaitingReview: number
  succeeded: number
  failed: number
  canceled: number
  interrupted: number
}

export type InvoiceBatchStatus =
  | "queued"
  | "processing"
  | "awaiting_review"
  | "completed"
  | "completed_with_errors"

export interface InvoiceBatchDetail {
  id: string
  status: InvoiceBatchStatus
  forcedType: number | null
  createdAt: string
  completedAt: string | null
  counts: InvoiceBatchCounts
  tasks: InvoiceTask[]
}

export type InvoiceBatchSummary = Omit<InvoiceBatchDetail, "tasks">

export interface InvoiceBatchesPage {
  items: InvoiceBatchSummary[]
  total: number
  page: number
  pageSize: number
}

export interface InvoiceTaskUpdateEvent {
  task: InvoiceTask
  batch: InvoiceBatchDetail
}

export type InvoiceSortBy = "invoice_date" | "total_amount" | "created_at"

export interface InvoicesQuery {
  page?: number
  pageSize?: number
  search?: string
  subtype?: string
  dateFrom?: string
  dateTo?: string
  duplicateOnly?: boolean
  sortBy?: InvoiceSortBy
  sortDirection?: SortDirection
}

export interface InvoiceListItem {
  id: string
  typeId: number
  subtype: string
  subtypeDescription: string
  invoiceCode: string | null
  invoiceNumber: string | null
  invoiceDate: string | null
  buyerName: string | null
  sellerName: string | null
  totalFen: number | null
  currency: string
  possibleDuplicate: boolean
  createdAt: string
}

export interface InvoicesPage {
  items: InvoiceListItem[]
  total: number
  page: number
  pageSize: number
}

export interface InvoiceLineItem {
  id: number
  position: number
  name: string
  specification: string | null
  unit: string | null
  quantity: string | null
  unitPriceFen: number | null
  amountFen: number | null
  taxRate: string | null
  taxFen: number | null
}

export interface InvoiceDetail {
  id: string
  taskId: string
  typeId: number
  subtype: string
  typeDescription: string
  subtypeDescription: string
  invoiceCode: string | null
  invoiceNumber: string | null
  invoiceDate: string | null
  buyerName: string | null
  buyerTaxId: string | null
  sellerName: string | null
  sellerTaxId: string | null
  amountFen: number | null
  taxFen: number | null
  totalFen: number | null
  currency: string
  possibleDuplicate: boolean
  previewPath: string
  sourcePath: string
  originalFileName: string
  pageStart: number
  pageEnd: number
  rawInvoiceJson: string
  rawResponseJson: string
  requestId: string | null
  createdAt: string
  updatedAt: string
  items: InvoiceLineItem[]
}

export interface UpdateInvoiceRequest {
  id: string
  invoiceCode: string | null
  invoiceNumber: string | null
  invoiceDate: string | null
  buyerName: string | null
  buyerTaxId: string | null
  sellerName: string | null
  sellerTaxId: string | null
  amountFen: number | null
  taxFen: number | null
  totalFen: number | null
  currency: string
}

export interface DeleteInvoiceResult {
  id: string
  deleted: boolean
  errorCode: string | null
  errorMessage: string | null
}

export interface DeleteInvoicesResult { results: DeleteInvoiceResult[] }
