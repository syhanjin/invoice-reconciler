import { cleanup, render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import type { AppSettings, OrderImportLogsPage, OrderImportPool } from "@/lib/types"

const apiMocks = vi.hoisted(() => ({
  getOrderImportPool: vi.fn(),
  listOrderImportLogs: vi.fn(),
  retryOrderImportTask: vi.fn(),
  retryFailedOrderImportTasks: vi.fn(),
  deleteOrderImportTask: vi.fn(),
  submitOrderRecognitionBatch: vi.fn(),
  cancelOrderRecognitionBatch: vi.fn(),
  listInvoiceBatches: vi.fn(),
}))

vi.mock("@/lib/api", () => ({
  api: apiMocks,
  errorMessage: (error: unknown) => error instanceof Error ? error.message : "操作失败",
}))

import ImportTasksPage from "./import-tasks-page"

const settings: AppSettings = {
  orderRecognitionProvider: "aliyun_batch",
  ollamaBaseUrl: "http://127.0.0.1:11434",
  ollamaModel: "qwen3-vl:4b",
  openaiBaseUrl: "https://example.com/v1",
  openaiModel: "vision-model",
  openaiApiKeyMasked: null,
  openaiApiKeyConfigured: false,
  aliyunBaseUrl: "https://dashscope.aliyuncs.com/compatible-mode/v1",
  aliyunModel: "qwen-vl-max",
  aliyunApiKeyMasked: "sk-****7890",
  aliyunApiKeyConfigured: true,
  aliyunEnableThinking: false,
  maxConcurrency: 3,
  tencentSecretIdMasked: null,
  tencentCredentialsConfigured: false,
  invoiceOcrConcurrency: 3,
  matchMaxOrders: 10,
  matchMaxCandidates: 100,
  settingsPath: "/data/settings.toml",
  permissionWarning: null,
}

const pool: OrderImportPool = {
  pending: 1,
  processing: 0,
  failed: 1,
  tasks: [
    {
      id: "task-pending",
      originalFileName: "pending-order.png",
      status: "pending",
      batchShortNumber: null,
      errorCode: null,
      errorMessage: null,
      retryCount: 0,
      createdAt: "2026-08-12T08:00:00Z",
      startedAt: null,
    },
    {
      id: "task-failed",
      originalFileName: "failed-order.png",
      status: "failed",
      batchShortNumber: null,
      errorCode: "recognition_failed",
      errorMessage: "识别失败",
      retryCount: 1,
      createdAt: "2026-08-12T08:01:00Z",
      startedAt: null,
    },
  ],
  batches: [{
    id: "batch-1",
    shortNumber: "B000001",
    status: "in_progress",
    model: "qwen-vl-max",
    taskCount: 2,
    errorMessage: null,
    createdAt: "2026-08-12T08:02:00Z",
    completedAt: null,
  }],
}

const logs: OrderImportLogsPage = {
  items: [{
    taskId: "task-completed",
    orderId: "order-1",
    originalFileName: "completed-order.png",
    provider: "aliyun_batch",
    model: "qwen-vl-max",
    batchShortNumber: "B000000",
    warningMessage: null,
    completedAt: "2026-08-12T08:03:00Z",
  }],
  total: 1,
  page: 1,
  pageSize: 20,
}

function renderPage(overrides: Partial<AppSettings> = {}) {
  return render(
    <ImportTasksPage
      settings={{ ...settings, ...overrides }}
      orderRefreshToken={0}
      activeInvoiceBatch={null}
      selectedInvoiceBatchId={null}
      onSelectedInvoiceBatchChange={vi.fn()}
      onInvoiceBatchChange={vi.fn()}
    />,
  )
}

describe("ImportTasksPage", () => {
  afterEach(cleanup)

  beforeEach(() => {
    vi.clearAllMocks()
    apiMocks.getOrderImportPool.mockResolvedValue(pool)
    apiMocks.listOrderImportLogs.mockResolvedValue(logs)
    apiMocks.retryOrderImportTask.mockResolvedValue(true)
    apiMocks.retryFailedOrderImportTasks.mockResolvedValue(1)
    apiMocks.deleteOrderImportTask.mockResolvedValue(undefined)
    apiMocks.submitOrderRecognitionBatch.mockResolvedValue("batch-2")
    apiMocks.cancelOrderRecognitionBatch.mockResolvedValue(undefined)
  })

  it("shows the live pool beside immutable completion logs", async () => {
    renderPage()

    expect((await screen.findAllByText("pending-order.png")).length).toBeGreaterThan(0)
    expect(screen.getAllByText("failed-order.png").length).toBeGreaterThan(0)
    expect(screen.getAllByText("completed-order.png").length).toBeGreaterThan(0)
    expect(screen.getAllByText("B000001").length).toBeGreaterThan(0)
    expect(apiMocks.getOrderImportPool).toHaveBeenCalled()
    expect(apiMocks.listOrderImportLogs).toHaveBeenCalledWith({ page: 1, pageSize: 20 })
  })

  it("submits every currently pending task as a new Aliyun batch", async () => {
    const user = userEvent.setup()
    renderPage()
    await screen.findAllByText("pending-order.png")

    await user.click(screen.getAllByRole("button", { name: "提交批次" })[0])

    await waitFor(() => expect(apiMocks.submitOrderRecognitionBatch).toHaveBeenCalledTimes(1))
  })

  it("allows a failed task to be retried without creating a batch", async () => {
    const user = userEvent.setup()
    renderPage({ orderRecognitionProvider: "ollama" })
    await screen.findAllByText("failed-order.png")

    expect(screen.queryByRole("button", { name: "提交批次" })).not.toBeInTheDocument()
    await user.click(screen.getAllByTitle("重试")[0])

    await waitFor(() => expect(apiMocks.retryOrderImportTask).toHaveBeenCalledWith("task-failed"))
  })

  it("cancels an active remote batch by its durable id", async () => {
    const user = userEvent.setup()
    renderPage()
    await screen.findAllByText("B000001")

    await user.click(screen.getAllByTitle("取消批次 B000001")[0])

    await waitFor(() => expect(apiMocks.cancelOrderRecognitionBatch).toHaveBeenCalledWith("batch-1"))
  })

  it("requires confirmation before deleting a pool task and its image", async () => {
    const user = userEvent.setup()
    renderPage()
    await screen.findAllByText("pending-order.png")

    await user.click(screen.getAllByTitle("删除任务")[0])
    expect(apiMocks.deleteOrderImportTask).not.toHaveBeenCalled()
    expect(screen.getByRole("heading", { name: "删除识别任务？" })).toBeInTheDocument()

    await user.click(screen.getByRole("button", { name: "删除" }))

    await waitFor(() => expect(apiMocks.deleteOrderImportTask).toHaveBeenCalledWith("task-pending"))
  })

  it("keeps the delete confirmation open when deleting the image fails", async () => {
    const user = userEvent.setup()
    apiMocks.deleteOrderImportTask.mockRejectedValue(new Error("删除失败"))
    renderPage()
    await screen.findAllByText("pending-order.png")

    await user.click(screen.getAllByTitle("删除任务")[0])
    await user.click(screen.getByRole("button", { name: "删除" }))

    await waitFor(() => expect(apiMocks.deleteOrderImportTask).toHaveBeenCalledWith("task-pending"))
    expect(screen.getByRole("heading", { name: "删除识别任务？" })).toBeInTheDocument()
  })
})
