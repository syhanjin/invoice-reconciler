import { cleanup, render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import type { OrderImportLogsPage, OrderImportPool } from "@/lib/types"

const apiMocks = vi.hoisted(() => ({
  getOrderImportPool: vi.fn(),
  listOrderImportLogs: vi.fn(),
  retryOrderImportTask: vi.fn(),
  retryFailedOrderImportTasks: vi.fn(),
  deleteOrderImportTask: vi.fn(),
  listInvoiceBatches: vi.fn(),
}))

vi.mock("@/lib/api", () => ({
  api: apiMocks,
  errorMessage: (error: unknown) => error instanceof Error ? error.message : "操作失败",
}))

import ImportTasksPage from "./import-tasks-page"

const pool: OrderImportPool = {
  pending: 1,
  processing: 0,
  failed: 1,
  tasks: [
    {
      id: "task-pending",
      originalFileName: "pending-order.png",
      status: "pending",
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
      errorCode: "recognition_failed",
      errorMessage: "识别失败",
      retryCount: 1,
      createdAt: "2026-08-12T08:01:00Z",
      startedAt: null,
    },
  ],
}

const logs: OrderImportLogsPage = {
  items: [{
    taskId: "task-completed",
    orderId: "order-1",
    originalFileName: "completed-order.png",
    provider: "aliyun_batch",
    model: "qwen-vl-max",
    warningMessage: null,
    completedAt: "2026-08-12T08:03:00Z",
  }],
  total: 1,
  page: 1,
  pageSize: 20,
}

function renderPage() {
  return render(
    <ImportTasksPage
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
  })

  it("shows the live pool beside immutable completion logs", async () => {
    renderPage()

    expect((await screen.findAllByText("pending-order.png")).length).toBeGreaterThan(0)
    expect(screen.getAllByText("failed-order.png").length).toBeGreaterThan(0)
    expect(screen.getAllByText("completed-order.png").length).toBeGreaterThan(0)
    expect(screen.getAllByText(/阿里云百炼（历史）/).length).toBeGreaterThan(0)
    expect(apiMocks.getOrderImportPool).toHaveBeenCalled()
    expect(apiMocks.listOrderImportLogs).toHaveBeenCalledWith({ page: 1, pageSize: 20 })
  })

  it("allows a failed task to be retried without creating a batch", async () => {
    const user = userEvent.setup()
    renderPage()
    await screen.findAllByText("failed-order.png")

    expect(screen.queryByRole("button", { name: "提交批次" })).not.toBeInTheDocument()
    await user.click(screen.getAllByTitle("重试")[0])

    await waitFor(() => expect(apiMocks.retryOrderImportTask).toHaveBeenCalledWith("task-failed"))
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
