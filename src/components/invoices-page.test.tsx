import { cleanup, render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

const mocks = vi.hoisted(() => ({
  open: vi.fn(),
  listInvoices: vi.fn(),
  prepareInvoiceBatch: vi.fn(),
  createInvoiceBatch: vi.fn(),
}))

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: mocks.open }))
vi.mock("@/lib/api", () => ({
  api: {
    listInvoices: mocks.listInvoices,
    prepareInvoiceBatch: mocks.prepareInvoiceBatch,
    createInvoiceBatch: mocks.createInvoiceBatch,
  },
  errorMessage: () => "操作失败",
}))
vi.mock("@/components/invoice-sheet", () => ({ default: () => null }))

import { InvoicesPage } from "./invoices-page"

const emptyPage = { items: [], total: 0, page: 1, pageSize: 20 }
const preview = {
  preflightToken: "token",
  selectedCount: 1,
  validCount: 1,
  duplicateCount: 0,
  invalidCount: 0,
  predictedRequests: 1,
  totalPdfPages: 0,
  forcedType: null,
  files: [{ path: "/tmp/invoice.png", originalFileName: "invoice.png", mediaType: "image/png", sizeBytes: 100, pdfPages: 0, status: "valid", message: null }],
}
const batch = { id: "batch", status: "queued", forcedType: null, createdAt: "2026-08-12T00:00:00Z", completedAt: null, counts: { total: 1, queued: 1, processing: 0, awaitingReview: 0, succeeded: 0, failed: 0, canceled: 0, interrupted: 0 }, tasks: [] }

describe("InvoicesPage", () => {
  afterEach(cleanup)
  beforeEach(() => {
    vi.clearAllMocks()
    mocks.listInvoices.mockResolvedValue(emptyPage)
    mocks.open.mockResolvedValue(["/tmp/invoice.png"])
    mocks.prepareInvoiceBatch.mockResolvedValue(preview)
    mocks.createInvoiceBatch.mockResolvedValue(batch)
  })

  it("preflights before creating a paid OCR batch", async () => {
    const user = userEvent.setup()
    const onBatchCreated = vi.fn()
    render(<InvoicesPage onBatchCreated={onBatchCreated} refreshToken={0} />)
    await screen.findByText("暂无发票")
    await user.click(screen.getByRole("button", { name: "导入发票" }))
    expect(mocks.prepareInvoiceBatch).toHaveBeenCalledWith(["/tmp/invoice.png"], null)
    expect(mocks.createInvoiceBatch).not.toHaveBeenCalled()
    await user.click(await screen.findByRole("button", { name: "确认并创建任务" }))
    await waitFor(() => expect(mocks.createInvoiceBatch).toHaveBeenCalledWith("token"))
    expect(onBatchCreated).toHaveBeenCalledWith(batch)
  })

  it("sends the selected subtype to the server query", async () => {
    const user = userEvent.setup()
    render(<InvoicesPage onBatchCreated={vi.fn()} refreshToken={0} />)
    await screen.findByText("暂无发票")
    await user.click(screen.getByRole("combobox", { name: "票种筛选" }))
    await user.click(await screen.findByRole("option", { name: "出租车发票" }))
    await waitFor(() => expect(mocks.listInvoices).toHaveBeenLastCalledWith(expect.objectContaining({ subtype: "TaxiTicket" })))
  })
})
