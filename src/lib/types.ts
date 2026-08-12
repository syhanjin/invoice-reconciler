export type NavigationView = "orders" | "importTasks" | "settings"

export interface AppSettings {
  ollamaBaseUrl: string
  ollamaModel: string
  maxConcurrency: number
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
