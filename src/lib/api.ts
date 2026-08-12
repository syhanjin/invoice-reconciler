import { convertFileSrc, invoke } from "@tauri-apps/api/core"
import { listen, type UnlistenFn } from "@tauri-apps/api/event"

import type {
  AppSettings,
  BootstrapState,
  DeleteOrdersResult,
  OrderImportLogsPage,
  OrderImportPool,
  OrderImportUpdateEvent,
  ImportOrderImagesResult,
  RecognizerStatus,
  OrderDetail,
  OrdersPage,
  OrdersQuery,
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
  ConfirmInvoiceMatchRequest,
  ConfirmInvoiceMatchResult,
  MatchableOrdersPage,
  MatchableOrdersQuery,
  MatchingPageData,
  MatchingQuery,
} from "@/lib/types"

export const api = {
  bootstrap: () => invoke<BootstrapState>("bootstrap"),
  getSettings: () => invoke<AppSettings>("get_settings"),
  saveSettings: (request: SaveSettingsRequest) =>
    invoke<AppSettings>("save_settings", { request }),
  checkOrderRecognizer: (request: SaveSettingsRequest) =>
    invoke<RecognizerStatus>("check_order_recognizer", { request }),
  importOrderImages: (imagePaths: string[]) =>
    invoke<ImportOrderImagesResult>("import_order_images", {
      request: { platform: "taobao", imagePaths },
    }),
  getOrderImportPool: () => invoke<OrderImportPool>("get_order_import_pool"),
  listOrderImportLogs: (query: { page?: number; pageSize?: number }) =>
    invoke<OrderImportLogsPage>("list_order_import_logs", { query }),
  retryOrderImportTask: (id: string) =>
    invoke<boolean>("retry_order_import_task", { request: { id } }),
  retryFailedOrderImportTasks: () =>
    invoke<number>("retry_failed_order_import_tasks"),
  deleteOrderImportTask: (id: string) =>
    invoke<void>("delete_order_import_task", { request: { id } }),
  submitOrderRecognitionBatch: () =>
    invoke<string>("submit_order_recognition_batch"),
  cancelOrderRecognitionBatch: (id: string) =>
    invoke<void>("cancel_order_recognition_batch", { request: { id } }),
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
  getMatchingPage: (query: MatchingQuery) =>
    invoke<MatchingPageData>("get_matching_page", { query }),
  listMatchableOrders: (query: MatchableOrdersQuery) =>
    invoke<MatchableOrdersPage>("list_matchable_orders", { query }),
  confirmInvoiceMatch: (request: ConfirmInvoiceMatchRequest) =>
    invoke<ConfirmInvoiceMatchResult>("confirm_invoice_match", { request }),
  confirmInvoiceMatches: (matches: ConfirmInvoiceMatchRequest[]) =>
    invoke<ConfirmInvoiceMatchResult[]>("confirm_invoice_matches", { request: { matches } }),
  unconfirmInvoiceMatch: (id: string) =>
    invoke<void>("unconfirm_invoice_match", { request: { id } }),
  listenForOrderImportUpdates: (
    handler: (payload: OrderImportUpdateEvent) => void,
  ): Promise<UnlistenFn> =>
    listen<OrderImportUpdateEvent>("order-import-updated", (event) => handler(event.payload)),
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
