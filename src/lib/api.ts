import { convertFileSrc, invoke } from "@tauri-apps/api/core"
import { listen, type UnlistenFn } from "@tauri-apps/api/event"

import type {
  AppSettings,
  BatchDetail,
  BootstrapState,
  ImportBatchesPage,
  ImportBatchesQuery,
  OllamaStatus,
  OrderDetail,
  OrdersPage,
  OrdersQuery,
  TaskUpdateEvent,
  UpdateOrderRequest,
} from "@/lib/types"

export const api = {
  bootstrap: () => invoke<BootstrapState>("bootstrap"),
  getSettings: () => invoke<AppSettings>("get_settings"),
  saveSettings: (request: AppSettings) =>
    invoke<AppSettings>("save_settings", { request }),
  checkOllama: (request: AppSettings) =>
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
  listenForTaskUpdates: (
    handler: (payload: TaskUpdateEvent) => void,
  ): Promise<UnlistenFn> =>
    listen<TaskUpdateEvent>("import-task-updated", (event) => handler(event.payload)),
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
