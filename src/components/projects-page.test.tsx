import { cleanup, render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

const mocks = vi.hoisted(() => ({
  open: vi.fn(),
  listProjects: vi.fn(),
  getProject: vi.fn(),
  createProject: vi.fn(),
  renameProject: vi.fn(),
  deleteProject: vi.fn(),
  reorderProjectInvoices: vi.fn(),
  exportProject: vi.fn(),
}))

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: mocks.open }))
vi.mock("@/lib/api", () => ({
  api: {
    listProjects: mocks.listProjects,
    getProject: mocks.getProject,
    createProject: mocks.createProject,
    renameProject: mocks.renameProject,
    deleteProject: mocks.deleteProject,
    reorderProjectInvoices: mocks.reorderProjectInvoices,
    exportProject: mocks.exportProject,
  },
  errorMessage: (error: { message?: string }) => error?.message ?? "操作失败",
  errorCode: (error: { code?: string }) => error?.code ?? null,
}))

import { ProjectsPage } from "./projects-page"

const summary = { id: "project-a", name: "差旅项目", invoiceCount: 2, confirmedCount: 2, unconfirmedCount: 0, actualAmountFen: 1800, exportable: true, blockingReason: null, createdAt: "2026-08-13T00:00:00Z", updatedAt: "2026-08-13T00:00:00Z" }
const invoice = (id: string, number: string, position: number) => ({ id, position, typeDescription: "增值税发票", invoiceCode: null, invoiceNumber: number, invoiceDate: "2026-08-13", buyerName: null, buyerTaxId: null, sellerName: "销售方", sellerTaxId: null, amountFen: 900, taxFen: 100, invoiceAmountFen: 1000, orderAmountFen: 900, actualAmountFen: 900, currency: "CNY", confirmed: true, fileAvailable: true, blockingReason: null, orders: [{ id: `order-${id}`, externalOrderNumber: `ORDER-${id}`, paidAmountFen: 900, productNames: ["差旅用品"] }] })
const detail = { ...summary, invoices: [invoice("invoice-a", "INV-A", 0), invoice("invoice-b", "INV-B", 1)] }

describe("ProjectsPage", () => {
  afterEach(cleanup)
  beforeEach(() => {
    vi.clearAllMocks()
    mocks.listProjects.mockResolvedValue({ items: [summary], total: 1, page: 1, pageSize: 20 })
    mocks.getProject.mockResolvedValue(detail)
    mocks.reorderProjectInvoices.mockResolvedValue({ ...detail, invoices: [detail.invoices[1], detail.invoices[0]] })
    mocks.createProject.mockResolvedValue({ ...detail, id: "project-new", name: "新项目" })
    mocks.renameProject.mockResolvedValue({ ...detail, name: "差旅项目 2026" })
    mocks.deleteProject.mockResolvedValue(undefined)
    mocks.open.mockResolvedValue("/tmp/export")
    mocks.exportProject.mockResolvedValue({ path: "/tmp/export/package.zip", invoiceCount: 2, actualAmountFen: 1800 })
  })

  it("persists the same member order used by project detail", async () => {
    const user = userEvent.setup()
    render(<ProjectsPage refreshToken={0} />)
    await screen.findByText("INV-A")
    await user.click(screen.getByRole("button", { name: "下移 INV-A" }))
    await waitFor(() => expect(mocks.reorderProjectInvoices).toHaveBeenCalledWith("project-a", ["invoice-b", "invoice-a"]))
  })

  it("creates a trimmed named project", async () => {
    const user = userEvent.setup()
    render(<ProjectsPage refreshToken={0} />)
    await user.click(await screen.findByRole("button", { name: "新建项目" }))
    const dialog = screen.getByRole("dialog")
    await user.type(within(dialog).getByPlaceholderText("输入项目名称"), "  新项目  ")
    await user.click(within(dialog).getByRole("button", { name: "保存" }))
    await waitFor(() => expect(mocks.createProject).toHaveBeenCalledWith("新项目"))
  })

  it("renames and then deletes membership-only project data", async () => {
    const user = userEvent.setup()
    render(<ProjectsPage refreshToken={0} />)
    await user.click(await screen.findByRole("button", { name: "重命名项目" }))
    let dialog = screen.getByRole("dialog")
    const name = within(dialog).getByPlaceholderText("输入项目名称")
    await user.clear(name)
    await user.type(name, "差旅项目 2026")
    await user.click(within(dialog).getByRole("button", { name: "保存" }))
    await waitFor(() => expect(mocks.renameProject).toHaveBeenCalledWith("project-a", "差旅项目 2026"))

    await user.click(screen.getByRole("button", { name: "删除项目" }))
    dialog = screen.getByRole("dialog")
    expect(within(dialog).getByText(/发票、订单、匹配关系和原文件都会保留/)).toBeInTheDocument()
    await user.click(within(dialog).getByRole("button", { name: "确认删除" }))
    await waitFor(() => expect(mocks.deleteProject).toHaveBeenCalledWith("project-a"))
  })

  it("confirms overwrite only after the backend reports an existing destination", async () => {
    const user = userEvent.setup()
    mocks.exportProject
      .mockRejectedValueOnce({ code: "destination_exists", message: "目标已存在" })
      .mockResolvedValueOnce({ path: "/tmp/export/package.zip", invoiceCount: 2, actualAmountFen: 1800 })
    render(<ProjectsPage refreshToken={0} />)
    await user.click(await screen.findByRole("button", { name: "导出材料包" }))
    expect(mocks.open).toHaveBeenCalledWith(expect.objectContaining({ directory: true }))
    expect(await screen.findByText("所选目录中已有同名 ZIP。确认后将安全替换旧文件；生成失败时保留旧文件。")).toBeInTheDocument()
    await user.click(screen.getByRole("button", { name: "覆盖导出" }))
    await waitFor(() => expect(mocks.exportProject).toHaveBeenLastCalledWith("project-a", "/tmp/export", true))
  })

  it("shows the backend blocking reason and disables empty project export", async () => {
    const empty = { ...summary, invoiceCount: 0, confirmedCount: 0, unconfirmedCount: 0, actualAmountFen: 0, exportable: false, blockingReason: "项目暂无发票" }
    mocks.listProjects.mockResolvedValue({ items: [empty], total: 1, page: 1, pageSize: 20 })
    mocks.getProject.mockResolvedValue({ ...empty, invoices: [] })
    render(<ProjectsPage refreshToken={0} />)
    expect(await screen.findByText("项目暂无发票")).toBeInTheDocument()
    expect(screen.getByRole("button", { name: "导出材料包" })).toBeDisabled()
  })
})
