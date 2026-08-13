import { cleanup, render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

const mocks = vi.hoisted(() => ({
  open: vi.fn(),
  listInvoices: vi.fn(),
  prepareInvoiceBatch: vi.fn(),
  createInvoiceBatch: vi.fn(),
  listProjects: vi.fn(),
  assignInvoicesToProject: vi.fn(),
  removeInvoicesFromProjects: vi.fn(),
}))

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: mocks.open }))
vi.mock("@/lib/api", () => ({
  api: {
    listInvoices: mocks.listInvoices,
    prepareInvoiceBatch: mocks.prepareInvoiceBatch,
    createInvoiceBatch: mocks.createInvoiceBatch,
    listProjects: mocks.listProjects,
    assignInvoicesToProject: mocks.assignInvoicesToProject,
    removeInvoicesFromProjects: mocks.removeInvoicesFromProjects,
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
    mocks.listProjects.mockResolvedValue({ items: [{ id: "project-a", name: "差旅报销", invoiceCount: 0, confirmedCount: 0, unconfirmedCount: 0, actualAmountFen: 0, exportable: false, blockingReason: "项目暂无发票", createdAt: "2026-08-13", updatedAt: "2026-08-13" }], total: 1, page: 1, pageSize: 100 })
    mocks.assignInvoicesToProject.mockResolvedValue({ assigned: true, conflicts: [] })
    mocks.removeInvoicesFromProjects.mockResolvedValue(undefined)
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

  it("filters the ledger by unassigned or specific project membership", async () => {
    const user = userEvent.setup()
    render(<InvoicesPage onBatchCreated={vi.fn()} refreshToken={0} />)
    await screen.findByText("暂无发票")
    await user.click(screen.getByRole("combobox", { name: "项目筛选" }))
    await user.click(await screen.findByRole("option", { name: "未分配项目" }))
    await waitFor(() => expect(mocks.listInvoices).toHaveBeenLastCalledWith(expect.objectContaining({ unassignedOnly: true, projectId: undefined })))
    await user.click(screen.getByRole("combobox", { name: "项目筛选" }))
    await user.click(await screen.findByRole("option", { name: "差旅报销" }))
    await waitFor(() => expect(mocks.listInvoices).toHaveBeenLastCalledWith(expect.objectContaining({ projectId: "project-a", unassignedOnly: false })))
  })

  it("assigns selected invoices to a project", async () => {
    const user = userEvent.setup()
    mocks.listInvoices.mockResolvedValue({
      items: [{ id: "invoice-a", typeId: 3, subtype: "VatInvoice", subtypeDescription: "增值税发票", invoiceCode: null, invoiceNumber: "INV-A", invoiceDate: "2026-08-13", buyerName: "购买方", sellerName: "销售方", totalFen: 1000, currency: "CNY", possibleDuplicate: false, createdAt: "2026-08-13T00:00:00Z", projectId: null, projectName: null }],
      total: 1, page: 1, pageSize: 20,
    })
    const onProjectsChanged = vi.fn()
    render(<InvoicesPage onBatchCreated={vi.fn()} refreshToken={0} onProjectsChanged={onProjectsChanged} />)
    await user.click(await screen.findByRole("checkbox", { name: "选择发票 INV-A" }))
    await user.click(screen.getByRole("button", { name: "分配项目" }))
    await user.click(screen.getByRole("combobox", { name: "目标项目" }))
    await user.click(await screen.findByRole("option", { name: "差旅报销" }))
    await user.click(screen.getByRole("button", { name: "确认分配" }))
    await waitFor(() => expect(mocks.assignInvoicesToProject).toHaveBeenCalledWith(["invoice-a"], "project-a", false))
    expect(onProjectsChanged).toHaveBeenCalled()
  })

  it("requires confirmation before reassigning an occupied invoice", async () => {
    const user = userEvent.setup()
    mocks.listInvoices.mockResolvedValue({
      items: [{ id: "invoice-a", typeId: 3, subtype: "VatInvoice", subtypeDescription: "增值税发票", invoiceCode: null, invoiceNumber: "INV-A", invoiceDate: "2026-08-13", buyerName: null, sellerName: null, totalFen: 1000, currency: "CNY", possibleDuplicate: false, createdAt: "2026-08-13T00:00:00Z", projectId: "old", projectName: "原项目" }],
      total: 1, page: 1, pageSize: 20,
    })
    mocks.assignInvoicesToProject
      .mockResolvedValueOnce({ assigned: false, conflicts: [{ invoiceId: "invoice-a", sourceProjectId: "old", sourceProjectName: "原项目" }] })
      .mockResolvedValueOnce({ assigned: true, conflicts: [] })
    render(<InvoicesPage onBatchCreated={vi.fn()} refreshToken={0} />)
    await user.click(await screen.findByRole("checkbox", { name: "选择发票 INV-A" }))
    await user.click(screen.getByRole("button", { name: "分配项目" }))
    await user.click(screen.getByRole("combobox", { name: "目标项目" }))
    await user.click(await screen.findByRole("option", { name: "差旅报销" }))
    await user.click(screen.getByRole("button", { name: "确认分配" }))
    expect(await screen.findByText("来自 原项目")).toBeInTheDocument()
    await user.click(screen.getByRole("button", { name: "确认改派" }))
    await waitFor(() => expect(mocks.assignInvoicesToProject).toHaveBeenLastCalledWith(["invoice-a"], "project-a", true))
  })

  it("removes selected invoices from their projects only after confirmation", async () => {
    const user = userEvent.setup()
    mocks.listInvoices.mockResolvedValue({
      items: [{ id: "invoice-a", typeId: 3, subtype: "VatInvoice", subtypeDescription: "增值税发票", invoiceCode: null, invoiceNumber: "INV-A", invoiceDate: "2026-08-13", buyerName: null, sellerName: null, totalFen: 1000, currency: "CNY", possibleDuplicate: false, createdAt: "2026-08-13T00:00:00Z", projectId: "project-a", projectName: "差旅报销" }],
      total: 1, page: 1, pageSize: 20,
    })
    render(<InvoicesPage onBatchCreated={vi.fn()} refreshToken={0} />)
    await user.click(await screen.findByRole("checkbox", { name: "选择发票 INV-A" }))
    await user.click(screen.getByRole("button", { name: "移出项目" }))
    expect(mocks.removeInvoicesFromProjects).not.toHaveBeenCalled()
    await user.click(screen.getByRole("button", { name: "确认移出" }))
    await waitFor(() => expect(mocks.removeInvoicesFromProjects).toHaveBeenCalledWith(["invoice-a"]))
  })
})
