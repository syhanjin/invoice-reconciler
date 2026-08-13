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
  LlmStatus,
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
  MatchSummaryUpdatedEvent,
  AssignInvoicesResult,
  ExportProjectResult,
  ProjectDetail,
  ProjectsPageData,
  ProjectsQuery,
} from "@/lib/types"

export const api = {
  bootstrap: () => invoke<BootstrapState>("bootstrap"),
  getSettings: () => invoke<AppSettings>("get_settings"),
  saveSettings: (request: SaveSettingsRequest) =>
    invoke<AppSettings>("save_settings", { request }),
  checkLlm: (request: SaveSettingsRequest) =>
    invoke<LlmStatus>("check_llm", { request }),
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
  retryMatchSummary: (id: string) =>
    invoke<void>("retry_match_summary", { request: { id } }),
  listProjects: (query: ProjectsQuery) =>
    invoke<ProjectsPageData>("list_projects", { query }),
  getProject: (id: string) =>
    invoke<ProjectDetail>("get_project", { request: { id } }),
  createProject: (name: string) =>
    invoke<ProjectDetail>("create_project", { request: { name } }),
  createProjectAndAssignInvoices: (name: string, invoiceIds: string[]) =>
    invoke<ProjectDetail>("create_project_and_assign_invoices", { request: { name, invoiceIds } }),
  renameProject: (id: string, name: string) =>
    invoke<ProjectDetail>("rename_project", { request: { id, name } }),
  deleteProject: (id: string) =>
    invoke<void>("delete_project", { request: { id } }),
  archiveProject: (id: string) =>
    invoke<void>("archive_project", { request: { id } }),
  unarchiveProject: (id: string) =>
    invoke<void>("unarchive_project", { request: { id } }),
  assignInvoicesToProject: (
    invoiceIds: string[],
    projectId: string,
    acceptReassignment: boolean,
  ) => invoke<AssignInvoicesResult>("assign_invoices_to_project", {
    request: { invoiceIds, projectId, acceptReassignment },
  }),
  reorderProjectInvoices: (projectId: string, invoiceIds: string[]) =>
    invoke<ProjectDetail>("reorder_project_invoices", { request: { projectId, invoiceIds } }),
  exportProject: (id: string, outputDirectory: string, overwrite: boolean) =>
    invoke<ExportProjectResult>("export_project", { request: { id, outputDirectory, overwrite } }),
  listenForOrderImportUpdates: (
    handler: (payload: OrderImportUpdateEvent) => void,
  ): Promise<UnlistenFn> =>
    listen<OrderImportUpdateEvent>("order-import-updated", (event) => handler(event.payload)),
  listenForInvoiceTaskUpdates: (
    handler: (payload: InvoiceTaskUpdateEvent) => void,
  ): Promise<UnlistenFn> =>
    listen<InvoiceTaskUpdateEvent>("invoice-task-updated", (event) => handler(event.payload)),
  listenForMatchSummaryUpdates: (
    handler: (payload: MatchSummaryUpdatedEvent) => void,
  ): Promise<UnlistenFn> =>
    listen<MatchSummaryUpdatedEvent>("match-summary-updated", (event) => handler(event.payload)),
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

export function errorCode(error: unknown): string | null {
  if (error && typeof error === "object") {
    const candidate = error as { code?: unknown }
    if (typeof candidate.code === "string") return candidate.code
  }
  return null
}
