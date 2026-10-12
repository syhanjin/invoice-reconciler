import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

const mocks = vi.hoisted(() => ({
  open: vi.fn(),
  listProjects: vi.fn(),
  assignInvoicesToProject: vi.fn(),
  getProject: vi.fn(),
  createProject: vi.fn(),
  renameProject: vi.fn(),
  deleteProject: vi.fn(),
  archiveProject: vi.fn(),
  unarchiveProject: vi.fn(),
  removeInvoicesFromProjects: vi.fn(),
  reorderProjectInvoices: vi.fn(),
  exportProject: vi.fn(),
  retryMatchSummary: vi.fn(),
}))

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: mocks.open }))
vi.mock("@/lib/api", () => ({
  api: {
    listProjects: mocks.listProjects,
    assignInvoicesToProject: mocks.assignInvoicesToProject,
    getProject: mocks.getProject,
    createProject: mocks.createProject,
    renameProject: mocks.renameProject,
    deleteProject: mocks.deleteProject,
    archiveProject: mocks.archiveProject,
    unarchiveProject: mocks.unarchiveProject,
    removeInvoicesFromProjects: mocks.removeInvoicesFromProjects,
    reorderProjectInvoices: mocks.reorderProjectInvoices,
    exportProject: mocks.exportProject,
    retryMatchSummary: mocks.retryMatchSummary,
  },
  errorMessage: (error: { message?: string }) => error?.message ?? "操作失败",
  errorCode: (error: { code?: string }) => error?.code ?? null,
}))

import { ProjectsPage } from "./projects-page"
import type { ProjectSummary } from "@/lib/types"

const summary = { id: "project-a", name: "差旅项目", invoiceCount: 2, confirmedCount: 2, unconfirmedCount: 0, summaryStatus: "succeeded" as const, summaryCounts: { total: 2, notGenerated: 0, pending: 0, processing: 0, succeeded: 2, failed: 0 }, actualAmountFen: 1800, exportable: true, blockingReason: null, createdAt: "2026-08-13T00:00:00Z", updatedAt: "2026-08-13T00:00:00Z", archivedAt: null }
const invoice = (id: string, number: string, position: number) => ({ id, displayName: "差旅用品", summaryStatus: "succeeded" as const, summarySource: "generated" as const, summaryError: null, summaryProvider: "ollama", summaryModel: "qwen", summaryGeneratedAt: "2026-08-13T00:00:00Z", position, typeDescription: "增值税发票", invoiceCode: null, invoiceNumber: number, invoiceDate: "2026-08-13", buyerName: null, buyerTaxId: null, sellerName: "销售方", sellerTaxId: null, amountFen: 900, taxFen: 100, invoiceAmountFen: 1000, orderAmountFen: 900, actualAmountFen: 900, currency: "CNY", confirmed: true, fileAvailable: true, blockingReason: null, orders: [{ id: `order-${id}`, externalOrderNumber: `ORDER-${id}`, paidAmountFen: 900, productNames: ["差旅用品"] }] })
const detail = { ...summary, invoices: [invoice("invoice-a", "INV-A", 0), invoice("invoice-b", "INV-B", 1)] }

type Clicker = { click: (element: Element) => Promise<void> }

const deferred = <T,>() => {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((res) => { resolve = res })
  return { promise, resolve }
}

const isTargetQuery = (query: { pageSize?: number }) => (query.pageSize ?? 20) >= 100

const projectSummary = (id: string, name: string, overrides: Partial<ProjectSummary> = {}): ProjectSummary => ({ ...summary, id, name, ...overrides })

const namedInvoice = (id: string, number: string, position: number, actualAmountFen: number) => ({
  ...invoice(id, number, position),
  displayName: `摘要-${number}`,
  actualAmountFen,
  orderAmountFen: actualAmountFen,
})

const moveCatalogue = {
  "invoice-a": namedInvoice("invoice-a", "INV-A", 0, 1100),
  "invoice-b": namedInvoice("invoice-b", "INV-B", 1, 1200),
  "invoice-c": namedInvoice("invoice-c", "INV-C", 2, 1300),
  "invoice-d": namedInvoice("invoice-d", "INV-D", 0, 1400),
  "invoice-e": namedInvoice("invoice-e", "INV-E", 0, 1500),
}

const sourceSummary = projectSummary("project-a", "差旅项目")
const targetSummary = projectSummary("project-b", "采购项目")

const memberNumbers = () =>
  screen.getAllByRole("checkbox", { name: /^选择发票 /, hidden: true }).map((box) => box.getAttribute("aria-label")!.replace("选择发票 ", ""))

const invoiceRow = (number: string) =>
  screen.getByRole("checkbox", { name: `选择发票 ${number}`, hidden: true }).closest("tr") as HTMLElement

const chooseMoveTarget = async (user: Clicker, name: string) => {
  const dialog = screen.getByRole("dialog")
  await user.click(within(dialog).getByRole("combobox", { name: "目标项目" }))
  await user.click(await screen.findByRole("option", { name }))
  return dialog
}

function moveWorld(options: { source?: string[]; target?: string[]; projects?: ProjectSummary[] } = {}) {
  const members: Record<string, string[]> = {
    "project-a": options.source ?? ["invoice-a", "invoice-b", "invoice-c"],
    "project-b": options.target ?? ["invoice-d"],
  }
  const projects = options.projects ?? [sourceSummary, targetSummary]
  const detailFor = (id: string) => {
    const project = projects.find((item) => item.id === id) ?? projectSummary(id, id)
    const invoices = (members[id] ?? []).map((invoiceId, position) => ({ ...moveCatalogue[invoiceId as keyof typeof moveCatalogue], position }))
    return { ...project, invoiceCount: invoices.length, confirmedCount: invoices.length, invoices }
  }
  const listQuery = (query: { page?: number; pageSize?: number; archived?: boolean }) => {
    const page = query.page ?? 1
    const pageSize = query.pageSize ?? 20
    const matches = query.archived ? projects.filter((item) => item.archivedAt) : projects.filter((item) => !item.archivedAt)
    return Promise.resolve({ items: matches.slice((page - 1) * pageSize, page * pageSize), total: matches.length, page, pageSize })
  }
  const apply = (invoiceIds: string[], projectId: string) => {
    members["project-a"] = (members["project-a"] ?? []).filter((id) => !invoiceIds.includes(id))
    members[projectId] = [...(members[projectId] ?? []), ...invoiceIds]
  }
  const wire = () => {
    mocks.listProjects.mockImplementation((query) => listQuery(query))
    mocks.getProject.mockImplementation((id: string) => Promise.resolve(detailFor(id)))
    mocks.assignInvoicesToProject.mockImplementation((invoiceIds: string[], projectId: string) => {
      apply(invoiceIds, projectId)
      return Promise.resolve({ assigned: true, conflicts: [] })
    })
  }
  return { members, detailFor, listQuery, apply, wire }
}

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
    mocks.archiveProject.mockResolvedValue(undefined)
    mocks.unarchiveProject.mockResolvedValue(undefined)
    mocks.removeInvoicesFromProjects.mockResolvedValue(undefined)
    mocks.assignInvoicesToProject.mockResolvedValue({ assigned: true, conflicts: [] })
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

  it("bulk removes selected members only after confirmation", async () => {
    const user = userEvent.setup()
    render(<ProjectsPage refreshToken={0} />)
    await screen.findByText("INV-A")

    const selectAll = screen.getByRole("checkbox", { name: "选择当前项目全部发票" })
    await user.click(selectAll)
    expect(selectAll).toHaveAttribute("aria-checked", "true")
    expect(screen.getByText("已选择 2 张")).toBeInTheDocument()
    await user.click(selectAll)
    expect(screen.queryByText("已选择 2 张")).not.toBeInTheDocument()
    await user.click(screen.getByRole("checkbox", { name: "选择发票 INV-A" }))
    expect(selectAll).toHaveAttribute("aria-checked", "mixed")
    await user.click(screen.getByRole("checkbox", { name: "选择发票 INV-B" }))
    expect(selectAll).toHaveAttribute("aria-checked", "true")
    expect(screen.getByText("已选择 2 张")).toBeInTheDocument()

    await user.click(screen.getByRole("button", { name: "移出项目" }))
    expect(mocks.removeInvoicesFromProjects).not.toHaveBeenCalled()
    const dialog = screen.getByRole("dialog")
    expect(within(dialog).getByText(/匹配关系、商品摘要和原文件都会保留/)).toBeInTheDocument()
    await user.click(within(dialog).getByRole("button", { name: "确认移出" }))

    await waitFor(() => expect(mocks.removeInvoicesFromProjects).toHaveBeenCalledWith(["invoice-a", "invoice-b"]))
    await waitFor(() => expect(screen.queryByText("已选择 2 张")).not.toBeInTheDocument())
    expect(mocks.getProject).toHaveBeenCalledTimes(2)
    expect(mocks.listProjects).toHaveBeenCalledTimes(2)
  })

  it("keeps selection and confirmation open when removal fails", async () => {
    const user = userEvent.setup()
    mocks.removeInvoicesFromProjects.mockRejectedValueOnce({ message: "归档项目不能修改成员" })
    render(<ProjectsPage refreshToken={0} />)
    await user.click(await screen.findByRole("checkbox", { name: "选择发票 INV-A" }))
    await user.click(screen.getByRole("button", { name: "移出项目" }))
    await user.click(screen.getByRole("button", { name: "确认移出" }))

    await waitFor(() => expect(mocks.removeInvoicesFromProjects).toHaveBeenCalledWith(["invoice-a"]))
    expect(screen.getByRole("dialog")).toHaveTextContent("选中的 1 张发票")
    await user.click(screen.getByRole("button", { name: "取消" }))
    expect(screen.getByRole("checkbox", { name: "选择发票 INV-A" })).toBeChecked()
  })

  it("clears member selection when switching projects", async () => {
    const user = userEvent.setup()
    const otherSummary = { ...summary, id: "project-b", name: "采购项目", invoiceCount: 1, confirmedCount: 1, summaryCounts: { ...summary.summaryCounts, total: 1, succeeded: 1 } }
    const otherDetail = { ...detail, ...otherSummary, invoices: [invoice("invoice-c", "INV-C", 0)] }
    mocks.listProjects.mockResolvedValue({ items: [summary, otherSummary], total: 2, page: 1, pageSize: 20 })
    mocks.getProject.mockImplementation((id: string) => Promise.resolve(id === "project-b" ? otherDetail : detail))
    render(<ProjectsPage refreshToken={0} />)

    await user.click(await screen.findByRole("checkbox", { name: "选择发票 INV-A" }))
    expect(screen.getByText("已选择 1 张")).toBeInTheDocument()
    await user.click(screen.getByRole("button", { name: /采购项目/ }))

    expect(await screen.findByRole("checkbox", { name: "选择发票 INV-C" })).not.toBeChecked()
    expect(screen.queryByText("已选择 1 张")).not.toBeInTheDocument()
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

  it("archives a project and can restore it from the archived view", async () => {
    const user = userEvent.setup()
    render(<ProjectsPage refreshToken={0} />)
    await user.click(await screen.findByRole("button", { name: "归档项目" }))
    await user.click(screen.getByRole("button", { name: "确认归档" }))
    await waitFor(() => expect(mocks.archiveProject).toHaveBeenCalledWith("project-a"))

    const archived = { ...summary, archivedAt: "2026-08-13T12:00:00Z" }
    mocks.listProjects.mockResolvedValue({ items: [archived], total: 1, page: 1, pageSize: 20 })
    mocks.getProject.mockResolvedValue({ ...detail, archivedAt: archived.archivedAt })
    await user.click(screen.getByRole("button", { name: "已归档" }))
    await user.click(await screen.findByRole("button", { name: "解除归档" }))
    await waitFor(() => expect(mocks.unarchiveProject).toHaveBeenCalledWith("project-a"))
    expect(screen.queryByRole("button", { name: "重命名项目" })).not.toBeInTheDocument()
    expect(screen.queryByRole("checkbox", { name: "选择当前项目全部发票" })).not.toBeInTheDocument()
    expect(screen.queryByRole("button", { name: "移出项目" })).not.toBeInTheDocument()
  })

  it("keeps export available with a fallback name and retries a failed summary", async () => {
    const user = userEvent.setup()
    const failedInvoice = {
      ...detail.invoices[0],
      displayName: "差旅用品",
      summaryStatus: "failed" as const,
      summaryError: "模型服务请求失败：离线",
    }
    const failedDetail = { ...detail, summaryStatus: "failed" as const, summaryCounts: { total: 1, notGenerated: 0, pending: 0, processing: 0, succeeded: 0, failed: 1 }, invoices: [failedInvoice] }
    mocks.getProject.mockResolvedValue(failedDetail)
    mocks.retryMatchSummary.mockResolvedValue(undefined)
    render(<ProjectsPage refreshToken={0} />)

    expect(await screen.findByText("商品摘要：部分失败")).toBeInTheDocument()
    expect(screen.getByRole("button", { name: "导出材料包" })).toBeEnabled()
    await user.click(screen.getByRole("button", { name: "重试商品摘要 INV-A" }))

    await waitFor(() => expect(mocks.retryMatchSummary).toHaveBeenCalledWith("invoice-a"))
  })

  it("moves selected invoices to the chosen project and keeps the source project in view", async () => {
    const user = userEvent.setup()
    const world = moveWorld()
    world.wire()
    render(<ProjectsPage refreshToken={0} />)
    await screen.findByText("INV-A")

    await user.click(screen.getByRole("checkbox", { name: "选择发票 INV-C" }))
    await user.click(screen.getByRole("checkbox", { name: "选择发票 INV-A" }))
    expect(screen.getByText("已选择 2 张")).toBeInTheDocument()

    await user.click(screen.getByRole("button", { name: "移到别的项目" }))
    const dialog = await chooseMoveTarget(user, "采购项目")
    await user.click(within(dialog).getByRole("button", { name: "确认移动" }))

    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument())
    await waitFor(() => expect(memberNumbers()).toEqual(["INV-B"]))
    expect(screen.queryByText("已选择 2 张")).not.toBeInTheDocument()
    expect(screen.getByRole("heading", { name: "差旅项目", hidden: true })).toBeInTheDocument()
    expect(screen.queryByText("INV-A")).not.toBeInTheDocument()
    expect(mocks.removeInvoicesFromProjects).not.toHaveBeenCalled()

    await user.click(screen.getByRole("button", { name: /采购项目/, hidden: true }))
    await waitFor(() => expect(memberNumbers()).toEqual(["INV-D", "INV-A", "INV-C"]))
    const movedRow = invoiceRow("INV-C")
    expect(within(movedRow).getByText("ORDER-invoice-c")).toBeInTheDocument()
    expect(within(movedRow).getByText("摘要-INV-C")).toBeInTheDocument()
    expect(movedRow.textContent).toContain("13.00")
  })

  it("keeps the dialog, target and selection when the move is rejected", async () => {
    const user = userEvent.setup()
    const world = moveWorld()
    world.wire()
    mocks.assignInvoicesToProject.mockRejectedValueOnce({ message: "目标项目已归档" })
    render(<ProjectsPage refreshToken={0} />)
    await screen.findByText("INV-A")

    await user.click(screen.getByRole("checkbox", { name: "选择发票 INV-A" }))
    await user.click(screen.getByRole("button", { name: "移到别的项目" }))
    const dialog = await chooseMoveTarget(user, "采购项目")
    await user.click(within(dialog).getByRole("button", { name: "确认移动" }))

    expect(await screen.findByText("目标项目已归档")).toBeInTheDocument()
    expect(memberNumbers()).toEqual(["INV-A", "INV-B", "INV-C"])
    expect(screen.getByText("已选择 1 张")).toBeInTheDocument()
    expect(within(dialog).getByRole("combobox", { name: "目标项目" })).toHaveTextContent("采购项目")
    expect(within(dialog).getByRole("button", { name: "确认移动" })).toBeEnabled()

    await user.click(within(dialog).getByRole("button", { name: "确认移动" }))
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument())
    await waitFor(() => expect(memberNumbers()).toEqual(["INV-B", "INV-C"]))
  })

  it("does not treat an unassigned result as a completed move and can retry", async () => {
    const user = userEvent.setup()
    const world = moveWorld()
    world.wire()
    mocks.assignInvoicesToProject.mockResolvedValueOnce({ assigned: false, conflicts: [] })
    render(<ProjectsPage refreshToken={0} />)
    await screen.findByText("INV-A")

    await user.click(screen.getByRole("checkbox", { name: "选择发票 INV-A" }))
    await user.click(screen.getByRole("button", { name: "移到别的项目" }))
    const dialog = await chooseMoveTarget(user, "采购项目")
    await user.click(within(dialog).getByRole("button", { name: "确认移动" }))

    expect(await screen.findByText("移动未完成，请刷新后重试")).toBeInTheDocument()
    expect(memberNumbers()).toEqual(["INV-A", "INV-B", "INV-C"])
    expect(screen.getByText("已选择 1 张")).toBeInTheDocument()

    await user.click(within(dialog).getByRole("button", { name: "取消" }))
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument())
    expect(memberNumbers()).toEqual(["INV-A", "INV-B", "INV-C"])
    expect(screen.getByText("已选择 1 张")).toBeInTheDocument()

    await user.click(screen.getByRole("button", { name: "移到别的项目", hidden: true }))
    await chooseMoveTarget(user, "采购项目")
    await user.click(screen.getByRole("button", { name: "确认移动" }))
    await waitFor(() => {
      expect(memberNumbers()).toEqual(["INV-B", "INV-C"])
      expect(screen.queryByText("已选择 1 张")).not.toBeInTheDocument()
    })
  })

  it("shows an empty target state when no other active project exists", async () => {
    const user = userEvent.setup()
    const world = moveWorld({ projects: [sourceSummary] })
    world.wire()
    render(<ProjectsPage refreshToken={0} />)
    await screen.findByText("INV-A")

    await user.click(screen.getByRole("checkbox", { name: "选择发票 INV-A" }))
    await user.click(screen.getByRole("button", { name: "移到别的项目" }))
    const dialog = screen.getByRole("dialog")

    expect(await within(dialog).findByText(/暂无其他进行中的项目/)).toBeInTheDocument()
    expect(within(dialog).getByRole("button", { name: "确认移动" })).toBeDisabled()

    await user.click(within(dialog).getByRole("button", { name: "取消" }))
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument())
    expect(screen.getByText("已选择 1 张")).toBeInTheDocument()
  })

  it("excludes the source project and archived projects from the move targets", async () => {
    const user = userEvent.setup()
    const archivedSummary = projectSummary("project-c", "归档项目", { archivedAt: "2026-08-13T12:00:00Z" })
    const world = moveWorld({ projects: [sourceSummary, targetSummary, archivedSummary] })
    world.wire()
    mocks.listProjects.mockImplementation((query) =>
      isTargetQuery(query)
        ? Promise.resolve({ items: [sourceSummary, targetSummary, archivedSummary], total: 3, page: query.page ?? 1, pageSize: query.pageSize })
        : world.listQuery(query),
    )
    render(<ProjectsPage refreshToken={0} />)
    await screen.findByText("INV-A")

    await user.click(screen.getByRole("checkbox", { name: "选择发票 INV-A" }))
    await user.click(screen.getByRole("button", { name: "移到别的项目" }))
    const dialog = screen.getByRole("dialog")
    await user.click(await within(dialog).findByRole("combobox", { name: "目标项目" }))

    expect(await screen.findByRole("option", { name: "采购项目" })).toBeInTheDocument()
    expect(screen.queryByRole("option", { name: "归档项目" })).not.toBeInTheDocument()
    expect(screen.queryByRole("option", { name: "差旅项目" })).not.toBeInTheDocument()
  })

  it("loads move targets past the backend's hundred-item page", async () => {
    const user = userEvent.setup()
    const world = moveWorld({ projects: [sourceSummary] })
    world.wire()
    const candidates = Array.from({ length: 100 }, (_, index) => projectSummary(`candidate-${index}`, `候选项目 ${index + 1}`))
    mocks.listProjects.mockImplementation((query) =>
      isTargetQuery(query)
        ? Promise.resolve({ items: (query.page ?? 1) === 1 ? candidates : [targetSummary], total: 101, page: query.page ?? 1, pageSize: query.pageSize })
        : world.listQuery(query),
    )
    render(<ProjectsPage refreshToken={0} />)
    await screen.findByText("INV-A")

    await user.click(screen.getByRole("checkbox", { name: "选择发票 INV-A" }))
    await user.click(screen.getByRole("button", { name: "移到别的项目" }))
    const dialog = screen.getByRole("dialog")
    await chooseMoveTarget(user, "采购项目")
    await user.click(within(dialog).getByRole("button", { name: "确认移动" }))

    await waitFor(() => expect(memberNumbers()).toEqual(["INV-B", "INV-C"]))
    expect(world.members["project-b"]).toEqual(["invoice-d", "invoice-a"])
  })

  it("surfaces a failed target load and recovers on manual retry", async () => {
    const user = userEvent.setup()
    const world = moveWorld()
    world.wire()
    mocks.listProjects.mockImplementation((query) => (isTargetQuery(query) ? Promise.reject({ message: "读取项目失败" }) : world.listQuery(query)))
    render(<ProjectsPage refreshToken={0} />)
    await screen.findByText("INV-A")

    await user.click(screen.getByRole("checkbox", { name: "选择发票 INV-A" }))
    await user.click(screen.getByRole("button", { name: "移到别的项目" }))
    const dialog = screen.getByRole("dialog")

    expect(await within(dialog).findByText("读取项目失败")).toBeInTheDocument()
    expect(within(dialog).getByRole("button", { name: "确认移动" })).toBeDisabled()

    world.wire()
    await user.click(within(dialog).getByRole("button", { name: "重试" }))
    await chooseMoveTarget(user, "采购项目")
    expect(within(dialog).getByRole("button", { name: "确认移动" })).toBeEnabled()
    await user.click(within(dialog).getByRole("button", { name: "确认移动" }))

    await waitFor(() => expect(memberNumbers()).toEqual(["INV-B", "INV-C"]))
  })

  it("keeps the source project selected when it falls outside the refreshed list", async () => {
    const user = userEvent.setup()
    const fillers = Array.from({ length: 20 }, (_, index) => projectSummary(`filler-${index}`, `填充项目 ${index + 1}`))
    const members: Record<string, string[]> = { "project-a": ["invoice-a", "invoice-b", "invoice-c"], "project-b": ["invoice-d"] }
    const detailFor = (id: string) => {
      const project = [sourceSummary, targetSummary, ...fillers].find((item) => item.id === id) ?? projectSummary(id, id)
      const invoices = members[id]
        ? members[id].map((invoiceId, position) => ({ ...moveCatalogue[invoiceId as keyof typeof moveCatalogue], position }))
        : [namedInvoice(`${id}-invoice`, `INV-${id}`, 0, 1000)]
      return { ...project, invoiceCount: invoices.length, confirmedCount: invoices.length, invoices }
    }
    let moved = false
    mocks.getProject.mockImplementation((id: string) => Promise.resolve(detailFor(id)))
    mocks.listProjects.mockImplementation((query) => {
      const page = query.page ?? 1
      if (isTargetQuery(query)) return Promise.resolve({ items: [sourceSummary, targetSummary], total: 2, page, pageSize: query.pageSize })
      if (!moved) return Promise.resolve({ items: page === 2 ? [sourceSummary] : fillers, total: 21, page, pageSize: 20 })
      return Promise.resolve({ items: page === 1 ? fillers : [], total: 20, page, pageSize: 20 })
    })
    mocks.assignInvoicesToProject.mockImplementation((invoiceIds: string[]) => {
      members["project-a"] = members["project-a"].filter((id) => !invoiceIds.includes(id))
      members["project-b"] = [...members["project-b"], ...invoiceIds]
      moved = true
      return Promise.resolve({ assigned: true, conflicts: [] })
    })
    render(<ProjectsPage refreshToken={0} />)
    await screen.findByText("INV-filler-0")

    const [, nextPage] = within(screen.getByText(/共 21 个/).parentElement as HTMLElement).getAllByRole("button")
    await user.click(nextPage)
    expect(await screen.findByText("2 / 2")).toBeInTheDocument()
    await screen.findByText("INV-A")

    await user.click(screen.getByRole("checkbox", { name: "选择发票 INV-A" }))
    await user.click(screen.getByRole("button", { name: "移到别的项目" }))
    const dialog = await chooseMoveTarget(user, "采购项目")
    await user.click(within(dialog).getByRole("button", { name: "确认移动" }))

    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument())
    await waitFor(() => expect(screen.getByText("1 / 1")).toBeInTheDocument())
    expect(screen.getByRole("heading", { name: "差旅项目", hidden: true })).toBeInTheDocument()
    await waitFor(() => expect(memberNumbers()).toEqual(["INV-B", "INV-C"]))
    expect(screen.queryByText("已选择 1 张")).not.toBeInTheDocument()
    expect(screen.queryByRole("button", { name: /差旅项目/, hidden: true })).not.toBeInTheDocument()

    await user.click(screen.getByRole("button", { name: /填充项目 3/, hidden: true }))
    expect(await screen.findByRole("heading", { name: "填充项目 3", hidden: true })).toBeInTheDocument()
    expect(memberNumbers()).toEqual(["INV-filler-2"])
  })

  it("preserves the submitting source context through a background workspace refresh", async () => {
    const user = userEvent.setup()
    const world = moveWorld()
    world.wire()
    const pending = deferred<{ assigned: boolean; conflicts: unknown[] }>()
    mocks.assignInvoicesToProject.mockImplementationOnce((invoiceIds: string[], projectId: string) => {
      world.apply(invoiceIds, projectId)
      return pending.promise
    })
    const { rerender } = render(<ProjectsPage refreshToken={0} />)
    await user.click(await screen.findByRole("checkbox", { name: "选择发票 INV-A" }))
    await user.click(screen.getByRole("button", { name: "移到别的项目" }))
    const dialog = await chooseMoveTarget(user, "采购项目")
    await user.click(within(dialog).getByRole("button", { name: "确认移动" }))

    mocks.listProjects.mockResolvedValue({ items: [targetSummary], total: 1, page: 1, pageSize: 20 })
    await act(async () => { rerender(<ProjectsPage refreshToken={1} />) })
    expect(screen.getByRole("dialog")).toBeInTheDocument()
    expect(screen.getByRole("heading", { name: "差旅项目", hidden: true })).toBeInTheDocument()
    expect(screen.getByText("已选择 1 张")).toBeInTheDocument()
    expect(memberNumbers()).toEqual(["INV-A", "INV-B", "INV-C"])

    await act(async () => { pending.resolve({ assigned: true, conflicts: [] }) })
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument())
    expect(screen.getByRole("heading", { name: "差旅项目" })).toBeInTheDocument()
    expect(memberNumbers()).toEqual(["INV-B", "INV-C"])
  })

  it("locks conflicting actions while the move is being submitted", async () => {
    const user = userEvent.setup()
    const world = moveWorld()
    world.wire()
    const pending = deferred<{ assigned: boolean; conflicts: unknown[] }>()
    mocks.assignInvoicesToProject.mockImplementationOnce((invoiceIds: string[], projectId: string) => {
      world.apply(invoiceIds, projectId)
      return pending.promise
    })
    render(<ProjectsPage refreshToken={0} />)
    await screen.findByText("INV-A")

    await user.click(screen.getByRole("checkbox", { name: "选择发票 INV-A" }))
    await user.click(screen.getByRole("button", { name: "移到别的项目" }))
    const dialog = await chooseMoveTarget(user, "采购项目")
    await user.click(within(dialog).getByRole("button", { name: "确认移动" }))

    await waitFor(() => expect(within(dialog).getByRole("button", { name: "确认移动" })).toBeDisabled())
    expect(within(dialog).getByRole("button", { name: "取消" })).toBeDisabled()
    expect(within(dialog).getByRole("combobox", { name: "目标项目" })).toBeDisabled()
    expect(screen.getByRole("button", { name: "下移 INV-A", hidden: true })).toBeDisabled()
    expect(screen.getByRole("button", { name: "移出项目", hidden: true })).toBeDisabled()
    expect(screen.getByRole("button", { name: /采购项目/, hidden: true })).toBeDisabled()
    expect(screen.getByRole("button", { name: "新建项目", hidden: true })).toBeDisabled()
    expect(screen.getByRole("button", { name: "重命名项目", hidden: true })).toBeDisabled()
    expect(screen.getByRole("button", { name: "归档项目", hidden: true })).toBeDisabled()
    expect(screen.getByRole("button", { name: "删除项目", hidden: true })).toBeDisabled()
    expect(screen.getByRole("button", { name: "进行中", hidden: true })).toBeDisabled()
    expect(screen.getByRole("button", { name: "已归档", hidden: true })).toBeDisabled()
    expect(screen.getByPlaceholderText("搜索项目")).toBeDisabled()

    fireEvent.click(within(dialog).getByRole("button", { name: "确认移动" }))
    fireEvent.click(screen.getByRole("checkbox", { name: "选择发票 INV-A", hidden: true }))
    fireEvent.click(screen.getByRole("checkbox", { name: "选择当前项目全部发票", hidden: true }))
    fireEvent.click(screen.getByRole("button", { name: /采购项目/, hidden: true }))
    await user.keyboard("{Escape}")
    expect(mocks.assignInvoicesToProject).toHaveBeenCalledTimes(1)
    expect(screen.getByRole("dialog")).toBeInTheDocument()
    expect(screen.getByText("已选择 1 张")).toBeInTheDocument()
    expect(screen.getByRole("heading", { name: "差旅项目", hidden: true })).toBeInTheDocument()

    await act(async () => { pending.resolve({ assigned: true, conflicts: [] }) })
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument())
    await waitFor(() => expect(memberNumbers()).toEqual(["INV-B", "INV-C"]))
  })

  it("discards a stale target load when the user switches projects before confirming", async () => {
    const user = userEvent.setup()
    const otherSummary = projectSummary("project-c", "其他项目")
    const first = deferred<{ items: ProjectSummary[]; total: number; page: number; pageSize: number }>()
    const second = deferred<{ items: ProjectSummary[]; total: number; page: number; pageSize: number }>()
    let targetLoads = 0
    mocks.listProjects.mockImplementation((query) => {
      if (!isTargetQuery(query)) return Promise.resolve({ items: [sourceSummary, otherSummary], total: 2, page: 1, pageSize: 20 })
      targetLoads += 1
      return targetLoads === 1 ? first.promise : second.promise
    })
    mocks.getProject.mockImplementation((id: string) =>
      Promise.resolve({ ...(id === "project-a" ? sourceSummary : otherSummary), invoices: [id === "project-a" ? moveCatalogue["invoice-a"] : moveCatalogue["invoice-e"]] }),
    )
    render(<ProjectsPage refreshToken={0} />)
    await screen.findByText("INV-A")

    await user.click(screen.getByRole("checkbox", { name: "选择发票 INV-A" }))
    await user.click(screen.getByRole("button", { name: "移到别的项目" }))
    expect(await screen.findByText(/正在加载项目/)).toBeInTheDocument()

    fireEvent.click(screen.getByRole("button", { name: /其他项目/, hidden: true }))
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument())
    await screen.findByText("INV-E")

    await user.click(screen.getByRole("checkbox", { name: "选择发票 INV-E", hidden: true }))
    await user.click(screen.getByRole("button", { name: "移到别的项目", hidden: true }))
    const dialog = screen.getByRole("dialog")
    expect(await within(dialog).findByText(/正在加载项目/)).toBeInTheDocument()

    await act(async () => { second.resolve({ items: [otherSummary], total: 1, page: 1, pageSize: 100 }) })
    expect(await within(dialog).findByText(/暂无其他进行中的项目/)).toBeInTheDocument()

    await act(async () => { first.resolve({ items: [sourceSummary, targetSummary], total: 2, page: 1, pageSize: 100 }) })
    await waitFor(() => expect(within(dialog).getByRole("button", { name: "确认移动" })).toBeDisabled())
    expect(within(dialog).getByText(/暂无其他进行中的项目/)).toBeInTheDocument()
    expect(screen.queryByText("采购项目")).not.toBeInTheDocument()
  })

  it("closes the move dialog and hides the entry when the detail becomes archived", async () => {
    const user = userEvent.setup()
    const world = moveWorld()
    world.wire()
    const { rerender } = render(<ProjectsPage refreshToken={0} />)
    await screen.findByText("INV-A")

    await user.click(screen.getByRole("checkbox", { name: "选择发票 INV-A" }))
    await user.click(screen.getByRole("button", { name: "移到别的项目" }))
    expect(await chooseMoveTarget(user, "采购项目")).toBeInTheDocument()

    const archivedAt = "2026-08-13T12:00:00Z"
    mocks.listProjects.mockImplementation((query) => (isTargetQuery(query) ? world.listQuery(query) : Promise.resolve({ items: [{ ...sourceSummary, archivedAt }], total: 1, page: 1, pageSize: 20 })))
    mocks.getProject.mockResolvedValue({ ...world.detailFor("project-a"), archivedAt })
    rerender(<ProjectsPage refreshToken={1} />)

    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument())
    expect(screen.queryByRole("button", { name: "移到别的项目", hidden: true })).not.toBeInTheDocument()
    expect(screen.queryByRole("button", { name: "移出项目", hidden: true })).not.toBeInTheDocument()
    expect(screen.queryByText("已选择 1 张")).not.toBeInTheDocument()
  })
})
