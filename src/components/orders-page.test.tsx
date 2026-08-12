import { cleanup, render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import type { OrdersPage as OrdersPageData, OrdersQuery } from "@/lib/types"

const apiMocks = vi.hoisted(() => ({
  listOrders: vi.fn<(query: OrdersQuery) => Promise<OrdersPageData>>(),
  createImportBatch: vi.fn(),
}))

vi.mock("@tauri-apps/api/webview", () => ({
  getCurrentWebview: () => ({ onDragDropEvent: vi.fn().mockResolvedValue(() => undefined) }),
}))

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }))

vi.mock("@/lib/api", () => ({
  api: apiMocks,
  errorMessage: (error: unknown) => error instanceof Error ? error.message : "操作失败",
}))

vi.mock("@/components/order-sheet", () => ({ default: () => null }))

import { OrdersPage } from "./orders-page"

describe("OrdersPage query controls", () => {
  afterEach(cleanup)

  beforeEach(() => {
    vi.clearAllMocks()
    localStorage.clear()
    apiMocks.listOrders.mockImplementation(async (query) => ({
      items: [{
        id: "order-1",
        platform: "taobao",
        externalOrderNumber: "10001",
        paidAmountFen: 1299,
        orderedAt: "2026-08-11 10:00",
        shopName: "示例店铺",
        possibleDuplicate: false,
        createdAt: "2026-08-12T08:00:00Z",
        productSummary: "测试商品",
        productCount: 1,
      }],
      total: 41,
      page: query.page ?? 1,
      pageSize: query.pageSize ?? 20,
    }))
  })

  it("sends controlled server queries and resets paging after sorting", async () => {
    const user = userEvent.setup()
    render(<OrdersPage onBatchCreated={vi.fn()} refreshToken={0} />)

    await waitFor(() => expect(apiMocks.listOrders).toHaveBeenCalledWith({
      page: 1,
      pageSize: 20,
      search: undefined,
      duplicateOnly: false,
      sortBy: "created_at",
      sortDirection: "desc",
    }))
    await user.click(screen.getByTitle("下一页"))
    await waitFor(() => expect(apiMocks.listOrders).toHaveBeenCalledWith(expect.objectContaining({ page: 2 })))

    await user.click(screen.getByRole("button", { name: "店铺排序" }))
    await waitFor(() => expect(apiMocks.listOrders).toHaveBeenCalledWith({
      page: 1,
      pageSize: 20,
      search: undefined,
      duplicateOnly: false,
      sortBy: "shop_name",
      sortDirection: "asc",
    }))
  })

  it("combines search, duplicate filtering, and page-size changes", async () => {
    const user = userEvent.setup()
    render(<OrdersPage onBatchCreated={vi.fn()} refreshToken={0} />)
    await screen.findByText("10001")

    const search = screen.getByPlaceholderText("搜索订单号、商品或店铺")
    await user.type(search, "  测试  {Enter}")
    await user.click(screen.getByRole("checkbox", { name: "仅可能重复" }))
    await user.click(screen.getByRole("combobox", { name: "每页条数" }))
    await user.click(await screen.findByRole("option", { name: "50 条" }))

    await waitFor(() => expect(apiMocks.listOrders).toHaveBeenCalledWith({
      page: 1,
      pageSize: 50,
      search: "测试",
      duplicateOnly: true,
      sortBy: "created_at",
      sortDirection: "desc",
    }))
  })

  it("corrects a page that becomes invalid after results shrink", async () => {
    const user = userEvent.setup()
    const view = render(<OrdersPage onBatchCreated={vi.fn()} refreshToken={0} />)
    await screen.findByText("10001")
    await user.click(screen.getByTitle("末页"))
    await waitFor(() => expect(apiMocks.listOrders).toHaveBeenCalledWith(expect.objectContaining({ page: 3 })))

    apiMocks.listOrders.mockImplementation(async (query) => ({
      items: [],
      total: 1,
      page: query.page ?? 1,
      pageSize: query.pageSize ?? 20,
    }))
    view.rerender(<OrdersPage onBatchCreated={vi.fn()} refreshToken={1} />)

    await waitFor(() => expect(apiMocks.listOrders).toHaveBeenCalledWith(expect.objectContaining({ page: 1 })))
  })

  it("keeps the last successful rows when a reload fails", async () => {
    const user = userEvent.setup()
    render(<OrdersPage onBatchCreated={vi.fn()} refreshToken={0} />)
    await screen.findByText("10001")

    apiMocks.listOrders.mockRejectedValueOnce(new Error("数据库不可用"))
    await user.click(screen.getByRole("checkbox", { name: "仅可能重复" }))

    await waitFor(() => expect(apiMocks.listOrders).toHaveBeenCalledWith(expect.objectContaining({ duplicateOnly: true })))
    expect(screen.getByText("10001")).toBeInTheDocument()
  })
})
