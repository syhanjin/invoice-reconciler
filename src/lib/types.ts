export type NavigationView = "orders" | "invoices" | "matching" | "projects" | "importTasks" | "settings"

export interface AppSettings {
  llmProvider: LlmProvider
  ollamaBaseUrl: string
  ollamaModel: string
  openaiBaseUrl: string
  openaiModel: string
  openaiApiKeyMasked: string | null
  openaiApiKeyConfigured: boolean
  orderRecognitionMaxConcurrency: number
  summaryGenerationMode: SummaryGenerationMode
  summaryGenerationMaxConcurrency: number
  tencentSecretIdMasked: string | null
  tencentCredentialsConfigured: boolean
  invoiceOcrConcurrency: number
  matchMaxOrders: number
  matchMaxCandidates: number
  settingsPath: string
  permissionWarning: string | null
}

export interface SaveSettingsRequest {
  llmProvider: LlmProvider
  ollamaBaseUrl: string
  ollamaModel: string
  openaiBaseUrl: string
  openaiModel: string
  openaiApiKey: string | null
  clearOpenaiApiKey: boolean
  orderRecognitionMaxConcurrency: number
  summaryGenerationMode: SummaryGenerationMode
  summaryGenerationMaxConcurrency: number
  tencentSecretId: string | null
  tencentSecretKey: string | null
  clearTencentCredentials: boolean
  invoiceOcrConcurrency: number
  matchMaxOrders: number
  matchMaxCandidates: number
}

export type LlmProvider = "ollama" | "openai"
export type SummaryGenerationMode = "automatic" | "manual"

export type OrderImportTaskStatus =
  | "pending"
  | "processing"
  | "failed"

export interface OrderImportTask {
  id: string
  originalFileName: string
  status: OrderImportTaskStatus
  errorCode: string | null
  errorMessage: string | null
  retryCount: number
  createdAt: string
  startedAt: string | null
}

export interface OrderImportPool {
  tasks: OrderImportTask[]
  pending: number
  processing: number
  failed: number
}

export interface ImportOrderImagesResult {
  imported: number
  skipped: number
}

export interface OrderImportLog {
  taskId: string
  orderId: string
  originalFileName: string
  provider: string
  model: string
  warningMessage: string | null
  completedAt: string
}

export interface OrderImportLogsPage {
  items: OrderImportLog[]
  total: number
  page: number
  pageSize: number
}

export interface BootstrapState {
  settings: AppSettings
  latestInvoiceBatch: InvoiceBatchDetail | null
}

export interface OrderImportUpdateEvent {
  kind: string
  taskId: string | null
  orderId: string | null
}

export interface MatchSummaryUpdatedEvent {
  invoiceId: string
  status: MatchSummaryStatus
}

export interface LlmStatus {
  provider: LlmProvider
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
  matchStatus: LedgerMatchStatus
  projectId: string | null
  projectName: string | null
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
  matchStatus?: LedgerMatchStatus
  projectStatus?: ProjectMembershipStatus
  projectId?: string
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
  | "matchStatus"
  | "projectName"

export type LedgerMatchStatus = "unmatched" | "matched" | "confirmed"
export type ProjectMembershipStatus = "assigned" | "unassigned"

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
  isMatched: boolean
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
  matchStatus?: LedgerMatchStatus
  projectStatus?: ProjectMembershipStatus
  projectId?: string
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
  projectId: string | null
  projectName: string | null
  matchStatus: LedgerMatchStatus
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
  isMatched: boolean
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

export type MatchingStatus = "suggested" | "conflict" | "unmatched" | "ineligible" | "confirmed"

export interface MatchOrderSummary {
  id: string
  externalOrderNumber: string | null
  paidAmountFen: number
  orderedAt: string | null
  shopName: string | null
  productSummary: string
}

export interface MatchCandidate {
  orderIds: string[]
  orders: MatchOrderSummary[]
  totalFen: number
  shopName: string | null
}

export interface MatchingInvoiceItem {
  id: string
  invoiceNumber: string | null
  subtypeDescription: string
  invoiceDate: string | null
  sellerName: string | null
  totalFen: number | null
  currency: string
  status: MatchingStatus
  reason: string | null
  candidates: MatchCandidate[]
  confirmedSource: "automatic" | "custom" | null
  confirmedAt: string | null
  displayName: string | null
  summaryStatus: MatchSummaryStatus | null
  summarySource: "generated" | "manual" | null
  summaryError: string | null
  summaryProvider: string | null
  summaryModel: string | null
  summaryGeneratedAt: string | null
}

export type MatchSummaryStatus = "not_generated" | "pending" | "processing" | "succeeded" | "failed"

export interface MatchSummaryCounts {
  total: number
  notGenerated: number
  pending: number
  processing: number
  succeeded: number
  failed: number
}

export interface MatchingSummary {
  total: number
  suggested: number
  conflict: number
  unmatched: number
  ineligible: number
  confirmed: number
}

export interface MatchingPageData {
  items: MatchingInvoiceItem[]
  summary: MatchingSummary
  summaryCounts: MatchSummaryCounts
  total: number
  page: number
  pageSize: number
}

export interface MatchingQuery {
  page?: number
  pageSize?: number
  search?: string
  status?: MatchingStatus | "all"
  summaryStatus?: MatchSummaryStatus | "all"
}

export interface MatchableOrderItem {
  id: string
  externalOrderNumber: string | null
  paidAmountFen: number | null
  orderedAt: string | null
  shopName: string | null
  productSummary: string
  matchedInvoiceId: string | null
  disabledReason: string | null
}

export interface MatchableOrdersPage {
  items: MatchableOrderItem[]
  total: number
  page: number
  pageSize: number
}

export interface MatchableOrdersQuery {
  page?: number
  pageSize?: number
  search?: string
  sortBy?: OrderSortBy
  sortDirection?: SortDirection
}

export interface ConfirmInvoiceMatchRequest {
  invoiceId: string
  orderIds: string[]
  source: "automatic" | "custom"
  acceptMismatches: boolean
}

export interface ConfirmInvoiceMatchResult {
  invoiceId: string
  confirmed: boolean
  differences: string[]
}

export interface DeleteInvoiceResult {
  id: string
  deleted: boolean
  errorCode: string | null
  errorMessage: string | null
}

export interface DeleteInvoicesResult { results: DeleteInvoiceResult[] }

export interface ProjectsQuery {
  page?: number
  pageSize?: number
  search?: string
  archived?: boolean
}

export interface ProjectSummary {
  id: string
  name: string
  invoiceCount: number
  confirmedCount: number
  unconfirmedCount: number
  summaryStatus: "not_applicable" | "not_generated" | "processing" | "failed" | "succeeded"
  summaryCounts: MatchSummaryCounts
  actualAmountFen: number
  exportable: boolean
  blockingReason: string | null
  createdAt: string
  updatedAt: string
  archivedAt: string | null
}

export interface ProjectsPageData {
  items: ProjectSummary[]
  total: number
  page: number
  pageSize: number
}

export interface ProjectOrderSummary {
  id: string
  externalOrderNumber: string | null
  paidAmountFen: number | null
  productNames: string[]
}

export interface ProjectInvoiceMember {
  id: string
  displayName: string
  summaryStatus: MatchSummaryStatus | null
  summarySource: "generated" | "manual" | null
  summaryError: string | null
  summaryProvider: string | null
  summaryModel: string | null
  summaryGeneratedAt: string | null
  position: number
  typeDescription: string
  invoiceCode: string | null
  invoiceNumber: string | null
  invoiceDate: string | null
  buyerName: string | null
  buyerTaxId: string | null
  sellerName: string | null
  sellerTaxId: string | null
  amountFen: number | null
  taxFen: number | null
  invoiceAmountFen: number | null
  orderAmountFen: number | null
  actualAmountFen: number | null
  currency: string
  confirmed: boolean
  fileAvailable: boolean
  blockingReason: string | null
  orders: ProjectOrderSummary[]
}

export interface ProjectDetail extends ProjectSummary {
  invoices: ProjectInvoiceMember[]
}

export interface MatchSummaryListItem {
  invoiceId: string
  invoiceNumber: string | null
  sellerName: string | null
  projectName: string | null
  displayName: string
  summaryStatus: MatchSummaryStatus
  summarySource: "generated" | "manual" | null
  summaryError: string | null
  summaryProvider: string | null
  summaryModel: string | null
  summaryGeneratedAt: string | null
  confirmedAt: string
}

export interface MatchSummariesQuery {
  page?: number
  pageSize?: number
  search?: string
  status?: MatchSummaryStatus | "all"
}

export interface MatchSummariesPage {
  items: MatchSummaryListItem[]
  counts: MatchSummaryCounts
  total: number
  page: number
  pageSize: number
}

export interface MatchSummaryInputInvoiceItem {
  name: string
  specification: string | null
  unit: string | null
  quantity: string | null
  unitPriceFen: number | null
  amountFen: number | null
  taxRate: string | null
  taxFen: number | null
}

export interface MatchSummaryInputOrder {
  shopName: string | null
  productNames: string[]
}

export interface MatchSummaryInput {
  sellerName: string | null
  invoiceItems: MatchSummaryInputInvoiceItem[]
  orders: MatchSummaryInputOrder[]
}

export type MatchSummaryAttemptStatus = "processing" | "succeeded" | "failed" | "interrupted" | "discarded" | "manual_edit"

export interface MatchSummaryAttemptItem {
  id: string
  invoiceId: string
  invoiceNumber: string | null
  input: MatchSummaryInput
  trigger: string
  provider: string | null
  model: string | null
  status: MatchSummaryAttemptStatus
  resultSummary: string | null
  errorMessage: string | null
  startedAt: string
  completedAt: string | null
}

export interface MatchSummaryAttemptsQuery {
  invoiceId?: string
  page?: number
  pageSize?: number
  search?: string
  status?: MatchSummaryAttemptStatus | "all"
}

export interface MatchSummaryAttemptsPage {
  items: MatchSummaryAttemptItem[]
  total: number
  page: number
  pageSize: number
}

export interface InvoiceProjectConflict {
  invoiceId: string
  sourceProjectId: string
  sourceProjectName: string
}

export interface AssignInvoicesResult {
  assigned: boolean
  conflicts: InvoiceProjectConflict[]
}

export interface ExportProjectResult {
  path: string
  invoiceCount: number
  actualAmountFen: number
}
