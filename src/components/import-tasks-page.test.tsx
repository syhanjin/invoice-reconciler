import { cleanup, render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { useState } from "react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import type { BatchDetail, ImportBatchesPage } from "@/lib/types"

const apiMocks = vi.hoisted(() => ({
  listImportBatches: vi.fn(),
  getBatch: vi.fn(),
  cancelTask: vi.fn(),
  cancelBatch: vi.fn(),
  retryTask: vi.fn(),
  retryFailedBatch: vi.fn(),
}))

vi.mock("@/lib/api", () => ({
  api: apiMocks,
  errorMessage: (error: unknown) => error instanceof Error ? error.message : "操作失败",
}))

import ImportTasksPage from "./import-tasks-page"

const detail: BatchDetail = {
  id: "batch-new",
  platform: "taobao",
  status: "completed_with_errors",
  createdAt: "2026-08-12T08:00:00Z",
  completedAt: "2026-08-12T08:01:00Z",
  counts: {
    total: 2,
    queued: 0,
    processing: 0,
    succeeded: 1,
    failed: 1,
    canceled: 0,
    skipped: 0,
    interrupted: 0,
  },
  tasks: [
    {
      id: "task-failed",
      batchId: "batch-new",
      originalFileName: "failed-order.png",
      managedImagePath: "images/task-failed.png",
      imageSha256: "hash",
      status: "failed",
      errorCode: "ocr_failed",
      errorMessage: "识别失败",
      warningMessage: null,
      retryCount: 0,
      createdAt: "2026-08-12T08:00:00Z",
      startedAt: "2026-08-12T08:00:01Z",
      completedAt: "2026-08-12T08:00:02Z",
    },
  ],
}

const history: ImportBatchesPage = {
  items: [{
    id: detail.id,
    platform: detail.platform,
    status: detail.status,
    createdAt: detail.createdAt,
    completedAt: detail.completedAt,
    counts: detail.counts,
  }],
  total: 21,
  page: 1,
  pageSize: 20,
}

function Harness() {
  const [selected, setSelected] = useState<string | null>(null)
  return (
    <ImportTasksPage
      activeBatch={null}
      selectedBatchId={selected}
      onSelectedBatchChange={setSelected}
      onBatchChange={vi.fn()}
      activeInvoiceBatch={null}
      selectedInvoiceBatchId={null}
      onSelectedInvoiceBatchChange={vi.fn()}
      onInvoiceBatchChange={vi.fn()}
    />
  )
}

describe("ImportTasksPage", () => {
  afterEach(cleanup)

  beforeEach(() => {
    vi.clearAllMocks()
    apiMocks.listImportBatches.mockResolvedValue(history)
    apiMocks.getBatch.mockResolvedValue(detail)
  })

  it("shows persisted order batches and loads the selected batch detail", async () => {
    render(<Harness />)

    expect(screen.getByRole("tab", { name: "订单" })).toHaveAttribute("data-active")
    expect(screen.getByRole("tab", { name: "发票" })).not.toHaveAttribute("aria-disabled", "true")
    expect(await screen.findByText("failed-order.png")).toBeInTheDocument()
    expect(apiMocks.listImportBatches).toHaveBeenCalledWith({ page: 1, pageSize: 20 })
    expect(apiMocks.getBatch).toHaveBeenCalledWith("batch-new")
  })

  it("loads the next page and resets to page one when page size changes", async () => {
    const user = userEvent.setup()
    render(<Harness />)
    await screen.findByText("failed-order.png")

    await user.click(screen.getByTitle("下一页"))
    await waitFor(() => {
      expect(apiMocks.listImportBatches).toHaveBeenCalledWith({ page: 2, pageSize: 20 })
    })

    await user.click(screen.getByRole("combobox", { name: "每页条数" }))
    await user.click(await screen.findByRole("option", { name: "50 条" }))
    await waitFor(() => {
      expect(apiMocks.listImportBatches).toHaveBeenCalledWith({ page: 1, pageSize: 50 })
    })
  })

  it("shows a retryable error state when persisted history cannot be loaded", async () => {
    apiMocks.listImportBatches
      .mockRejectedValueOnce(new Error("数据库不可用"))
      .mockResolvedValueOnce(history)
    const user = userEvent.setup()
    render(<Harness />)

    expect(await screen.findByText("导入历史加载失败")).toBeInTheDocument()
    await user.click(screen.getByRole("button", { name: "重新加载" }))

    expect(await screen.findByText("failed-order.png")).toBeInTheDocument()
    expect(apiMocks.listImportBatches).toHaveBeenCalledTimes(2)
  })
})
