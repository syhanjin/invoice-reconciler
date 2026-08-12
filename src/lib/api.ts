import { convertFileSrc, invoke } from "@tauri-apps/api/core"
import { listen, type UnlistenFn } from "@tauri-apps/api/event"

import type {
  AppSettings,
  BatchDetail,
  BootstrapState,
  DeleteOrdersResult,
  ImportBatchesPage,
  ImportBatchesQuery,
  OllamaStatus,
  OrderDetail,
  OrdersPage,
  OrdersQuery,
  TaskUpdateEvent,
  UpdateOrderRequest,
  SaveSettingsRequest,
  InvoiceBatchPreview,
  InvoiceBatchDetail,
  InvoiceBatchesPage,
  InvoiceTaskUpdateEvent,
  InvoicesPage,
  InvoicesQuery,
  InvoiceDetail,
  UpdateInvoiceRequest,
  DeleteInvoicesResult,
} from "@/lib/types"

export const api = {
  bootstrap: () => invoke<BootstrapState>("bootstrap"),
  getSettings: () => invoke<AppSettings>("get_settings"),
  saveSettings: (request: SaveSettingsRequest) =>
    invoke<AppSettings>("save_settings", { request }),
  checkOllama: (request: SaveSettingsRequest) =>
    invoke<OllamaStatus>("check_ollama", { request }),
  createImportBatch: (imagePaths: string[]) =>
    invoke<BatchDetail>("create_import_batch", {
      request: { platform: "taobao", imagePaths },
    }),
  getBatch: (id: string) => invoke<BatchDetail>("get_batch", { request: { id } }),
  listImportBatches: (query: ImportBatchesQuery) =>
    invoke<ImportBatchesPage>("list_import_batches", { query }),
  cancelTask: (id: string) => invoke<boolean>("cancel_task", { request: { id } }),
  cancelBatch: (id: string) => invoke<number>("cancel_batch", { request: { id } }),
  retryTask: (id: string) => invoke<BatchDetail>("retry_task", { request: { id } }),
  retryFailedBatch: (id: string) =>
    invoke<BatchDetail>("retry_failed_batch", { request: { id } }),
  listOrders: (query: OrdersQuery) => invoke<OrdersPage>("list_orders", { query }),
  getOrder: (id: string) => invoke<OrderDetail>("get_order", { request: { id } }),
  updateOrder: (request: UpdateOrderRequest) =>
    invoke<OrderDetail>("update_order", { request }),
  deleteOrder: (id: string) => invoke<void>("delete_order", { request: { id } }),
  deleteOrders: (ids: string[]) =>
    invoke<DeleteOrdersResult>("delete_orders", { request: { ids } }),
  prepareInvoiceBatch: (filePaths: string[], forcedType: number | null) =>
    invoke<InvoiceBatchPreview>("prepare_invoice_batch", { request: { filePaths, forcedType } }),
  createInvoiceBatch: (preflightToken: string) =>
    invoke<InvoiceBatchDetail>("create_invoice_batch", { request: { preflightToken } }),
  getInvoiceBatch: (id: string) => invoke<InvoiceBatchDetail>("get_invoice_batch", { request: { id } }),
  listInvoiceBatches: (query: { page?: number; pageSize?: number }) =>
    invoke<InvoiceBatchesPage>("list_invoice_batches", { query }),
  cancelInvoiceTask: (id: string) => invoke<boolean>("cancel_invoice_task", { request: { id } }),
  retryInvoiceTask: (id: string, forcedType: number | null) =>
    invoke<InvoiceBatchDetail>("retry_invoice_task", { request: { id, forcedType } }),
  confirmInvoiceSplit: (taskId: string, groups: number[][]) =>
    invoke<InvoiceBatchDetail>("confirm_invoice_split", { request: { taskId, groups } }),
  listInvoices: (query: InvoicesQuery) => invoke<InvoicesPage>("list_invoices", { query }),
  getInvoice: (id: string) => invoke<InvoiceDetail>("get_invoice", { request: { id } }),
  updateInvoice: (request: UpdateInvoiceRequest) =>
    invoke<InvoiceDetail>("update_invoice", { request }),
  deleteInvoice: (id: string) => invoke<void>("delete_invoice", { request: { id } }),
  deleteInvoices: (ids: string[]) =>
    invoke<DeleteInvoicesResult>("delete_invoices", { request: { ids } }),
  listenForTaskUpdates: (
    handler: (payload: TaskUpdateEvent) => void,
  ): Promise<UnlistenFn> =>
    listen<TaskUpdateEvent>("import-task-updated", (event) => handler(event.payload)),
  listenForInvoiceTaskUpdates: (
    handler: (payload: InvoiceTaskUpdateEvent) => void,
  ): Promise<UnlistenFn> =>
    listen<InvoiceTaskUpdateEvent>("invoice-task-updated", (event) => handler(event.payload)),
  imageUrl: (absolutePath: string) => convertFileSrc(absolutePath),
}

export function errorMessage(error: unknown): string {
  if (typeof error === "string") return error
  if (error && typeof error === "object") {
    const candidate = error as { message?: unknown }
    if (typeof candidate.message === "string") return candidate.message
  }
  return "操作失败，请稍后重试"
}
