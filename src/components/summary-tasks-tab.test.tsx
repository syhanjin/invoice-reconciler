import { cleanup, render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

const mocks = vi.hoisted(() => ({
  listMatchSummaries: vi.fn(),
  listMatchSummaryAttempts: vi.fn(),
  enqueueMatchSummary: vi.fn(),
  enqueueAllMatchSummaries: vi.fn(),
  updateMatchSummary: vi.fn(),
  listenForMatchSummaryUpdates: vi.fn(),
}))

vi.mock("@/lib/api", () => ({
  api: mocks,
  errorMessage: (error: unknown) => error instanceof Error ? error.message : "操作失败",
}))

import { SummaryTasksTab } from "./summary-tasks-tab"

describe("SummaryTasksTab", () => {
  afterEach(cleanup)

  beforeEach(() => {
    vi.clearAllMocks()
    mocks.listenForMatchSummaryUpdates.mockResolvedValue(vi.fn())
    mocks.enqueueMatchSummary.mockResolvedValue(undefined)
    mocks.enqueueAllMatchSummaries.mockResolvedValue(1)
    mocks.updateMatchSummary.mockResolvedValue(undefined)
    mocks.listMatchSummaries.mockResolvedValue({
      items: [{ invoiceId: "invoice-a", invoiceNumber: "INV-A", sellerName: "销售方", projectName: "测试项目", displayName: "数据线", summaryStatus: "succeeded", summarySource: "generated", summaryError: null, summaryProvider: "ollama", summaryModel: "qwen", summaryGeneratedAt: "2026-08-14T08:01:00Z", confirmedAt: "2026-08-14T08:00:00Z" }],
      counts: { total: 1, notGenerated: 0, pending: 0, processing: 0, succeeded: 1, failed: 0 },
      total: 1,
      page: 1,
      pageSize: 20,
    })
    mocks.listMatchSummaryAttempts.mockResolvedValue({
      items: [{ id: "attempt-a", invoiceId: "invoice-a", invoiceNumber: "INV-A", input: { sellerName: "销售方", invoiceItems: [{ name: "充电设备", specification: "USB-C", unit: null, quantity: "1", unitPriceFen: null, amountFen: 1000, taxRate: null, taxFen: null }], orders: [{ shopName: "店铺", productNames: ["数据线"] }] }, trigger: "automatic_confirmation", provider: "ollama", model: "qwen", status: "succeeded", resultSummary: "数据线", errorMessage: null, startedAt: "2026-08-14T08:00:00Z", completedAt: "2026-08-14T08:01:00Z" }],
      total: 1,
      page: 1,
      pageSize: 20,
    })
  })

  it("shows current summaries and structured attempt inputs", async () => {
    const user = userEvent.setup()
    render(<SummaryTasksTab />)

    expect((await screen.findAllByText(/INV-A · 数据线/)).length).toBeGreaterThan(0)
    await waitFor(() => expect(mocks.listMatchSummaryAttempts).toHaveBeenCalledWith({ invoiceId: "invoice-a", page: 1, pageSize: 20 }))
    await user.click(screen.getAllByText("详情")[0])
    expect(screen.getAllByText(/充电设备/).length).toBeGreaterThan(0)
    expect(screen.getAllByText(/店铺 · 数据线/).length).toBeGreaterThan(0)
  })

  it("queues all unfinished summaries and edits a completed summary", async () => {
    const user = userEvent.setup()
    mocks.listMatchSummaries.mockResolvedValue({
      items: [{ invoiceId: "invoice-a", invoiceNumber: "INV-A", sellerName: null, projectName: null, displayName: "数据线", summaryStatus: "succeeded", summarySource: "generated", summaryError: null, summaryProvider: "ollama", summaryModel: "qwen", summaryGeneratedAt: null, confirmedAt: "2026-08-14T08:00:00Z" }],
      counts: { total: 2, notGenerated: 1, pending: 0, processing: 0, succeeded: 1, failed: 0 }, total: 1, page: 1, pageSize: 20,
    })
    render(<SummaryTasksTab />)
    await screen.findAllByText(/INV-A · 数据线/)

    await user.click(screen.getAllByRole("button", { name: "生成/重试全部" })[0])
    await waitFor(() => expect(mocks.enqueueAllMatchSummaries).toHaveBeenCalled())

    await user.click(screen.getAllByTitle("编辑摘要")[0])
    const input = screen.getByRole("textbox", { name: "商品摘要" })
    await user.clear(input)
    await user.type(input, "充电线缆")
    await user.click(screen.getByRole("button", { name: "保存" }))
    await waitFor(() => expect(mocks.updateMatchSummary).toHaveBeenCalledWith("invoice-a", "充电线缆"))
  })

  it("keeps the editor open when a manual update fails", async () => {
    const user = userEvent.setup()
    mocks.updateMatchSummary.mockRejectedValue(new Error("保存失败"))
    render(<SummaryTasksTab />)
    await screen.findAllByText(/INV-A · 数据线/)

    await user.click(screen.getAllByTitle("编辑摘要")[0])
    const input = screen.getByRole("textbox", { name: "商品摘要" })
    await user.clear(input)
    await user.type(input, "充电线缆")
    await user.click(screen.getByRole("button", { name: "保存" }))

    await waitFor(() => expect(mocks.updateMatchSummary).toHaveBeenCalled())
    expect(screen.getByRole("textbox", { name: "商品摘要" })).toBeVisible()
  })
})
