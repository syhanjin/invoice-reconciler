import { cleanup, render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import type { MatchingPageData } from "@/lib/types"

const mocks = vi.hoisted(() => ({
  getMatchingPage: vi.fn(),
  listMatchableOrders: vi.fn(),
  confirmInvoiceMatch: vi.fn(),
  confirmInvoiceMatches: vi.fn(),
  unconfirmInvoiceMatch: vi.fn(),
  retryMatchSummary: vi.fn(),
}))

vi.mock("@/lib/api", () => ({
  api: mocks,
  errorMessage: (error: unknown) => error instanceof Error ? error.message : "操作失败",
}))
vi.mock("@/components/invoice-sheet", () => ({ default: () => null }))
vi.mock("@/components/order-sheet", () => ({ default: () => null }))

import { MatchingPage } from "./matching-page"

const candidate = (id: string, amount = 100) => ({
  orderIds: [id],
  orders: [{ id, externalOrderNumber: id, paidAmountFen: amount, orderedAt: "2026-08-01", shopName: "店铺", productSummary: "商品" }],
  totalFen: amount,
  shopName: "店铺",
})

const noSummary = {
  displayName: null,
  summaryStatus: null,
  summaryError: null,
  summaryProvider: null,
  summaryModel: null,
  summaryGeneratedAt: null,
} as const

const page: MatchingPageData = {
  items: [
    { ...noSummary, id: "invoice-a", invoiceNumber: "INV-A", subtypeDescription: "增值税发票", invoiceDate: "2026-08-10", sellerName: "销售方", totalFen: 100, currency: "CNY", status: "suggested", reason: null, candidates: [candidate("order-a")], confirmedSource: null, confirmedAt: null },
    { ...noSummary, id: "invoice-b", invoiceNumber: "INV-B", subtypeDescription: "增值税发票", invoiceDate: "2026-08-10", sellerName: "销售方", totalFen: 100, currency: "CNY", status: "conflict", reason: "疑似回购冲突：订单 2 条，发票 3 张，数量不一致", candidates: [candidate("order-b"), candidate("order-c")], confirmedSource: null, confirmedAt: null },
    { ...noSummary, id: "invoice-c", invoiceNumber: "INV-C", subtypeDescription: "增值税发票", invoiceDate: "2026-08-10", sellerName: "销售方", totalFen: 120, currency: "CNY", status: "unmatched", reason: "没有满足自动规则的订单组合", candidates: [], confirmedSource: null, confirmedAt: null },
  ],
  summary: { total: 3, suggested: 1, conflict: 1, unmatched: 1, ineligible: 0, confirmed: 0 },
  total: 3,
  page: 1,
  pageSize: 50,
}

describe("MatchingPage", () => {
  afterEach(cleanup)

  beforeEach(() => {
    vi.clearAllMocks()
    mocks.getMatchingPage.mockResolvedValue(page)
    mocks.confirmInvoiceMatches.mockResolvedValue([{ invoiceId: "invoice-a", confirmed: true, differences: [] }])
    mocks.confirmInvoiceMatch.mockResolvedValue({ invoiceId: "invoice-b", confirmed: true, differences: [] })
    mocks.listMatchableOrders.mockResolvedValue({
      items: [{ id: "custom-order", externalOrderNumber: "CUSTOM-1", paidAmountFen: 100, orderedAt: null, shopName: "另一店", productSummary: "商品", matchedInvoiceId: null, disabledReason: null }],
      total: 1,
      page: 1,
      pageSize: 20,
    })
  })

  it("shows the localized label for the active status filter", async () => {
    render(<MatchingPage refreshToken="0:0" />)
    await screen.findByText("INV-A")

    expect(screen.getByText("全部状态")).toBeInTheDocument()
  })

  it("shows the dedicated suspected-repurchase conflict reason", async () => {
    render(<MatchingPage refreshToken="0:0" />)

    expect(await screen.findByText("疑似回购冲突：订单 2 条，发票 3 张，数量不一致")).toBeInTheDocument()
  })

  it("bulk confirms checked conflict-free suggestions", async () => {
    const user = userEvent.setup()
    render(<MatchingPage refreshToken="0:0" />)
    await screen.findByText("INV-A")

    await user.click(screen.getByRole("checkbox", { name: "选择发票 INV-A" }))
    await user.click(screen.getByRole("button", { name: "确认选中的 1 项" }))

    await waitFor(() => expect(mocks.confirmInvoiceMatches).toHaveBeenCalledWith([{
      invoiceId: "invoice-a",
      orderIds: ["order-a"],
      source: "automatic",
      acceptMismatches: false,
    }]))
  })

  it("confirms the selected candidate for a conflict invoice", async () => {
    const user = userEvent.setup()
    render(<MatchingPage refreshToken="0:0" />)
    await screen.findByText("INV-B")

    await user.click(screen.getByRole("button", { name: "组合 2" }))
    const confirmButtons = screen.getAllByRole("button", { name: "确认组合" })
    await user.click(confirmButtons[1])

    await waitFor(() => expect(mocks.confirmInvoiceMatch).toHaveBeenCalledWith({
      invoiceId: "invoice-b",
      orderIds: ["order-c"],
      source: "automatic",
      acceptMismatches: false,
    }))
  })

  it("requires explicit acceptance for a mismatched custom combination", async () => {
    const user = userEvent.setup()
    mocks.confirmInvoiceMatch
      .mockResolvedValueOnce({ invoiceId: "invoice-c", confirmed: false, differences: ["订单合计与发票金额相差 -20 分"] })
      .mockResolvedValueOnce({ invoiceId: "invoice-c", confirmed: true, differences: ["订单合计与发票金额相差 -20 分"] })
    render(<MatchingPage refreshToken="0:0" />)
    await screen.findByText("INV-C")

    await user.click(screen.getByRole("button", { name: "自定义组合" }))
    const dialog = screen.getByRole("dialog")
    expect(dialog.style.width).toContain("min(1100px")
    expect(dialog.style.width).toContain("100vw")
    expect(dialog.style.width).toContain("32px")
    expect(dialog.style.maxWidth).toBe("none")
    await user.click(await screen.findByRole("checkbox", { name: "选择订单 CUSTOM-1" }))
    await user.click(screen.getByRole("button", { name: "检查并确认" }))
    expect(await screen.findByText("订单合计与发票金额相差 -20 分")).toBeInTheDocument()
    await user.click(screen.getByRole("button", { name: "接受差异并确认" }))

    await waitFor(() => expect(mocks.confirmInvoiceMatch).toHaveBeenLastCalledWith({
      invoiceId: "invoice-c",
      orderIds: ["custom-order"],
      source: "custom",
      acceptMismatches: true,
    }))
  })

  it("shows a fallback product name and retries a failed generated summary", async () => {
    const user = userEvent.setup()
    mocks.getMatchingPage.mockResolvedValue({
      ...page,
      items: [{
        ...page.items[0],
        status: "confirmed",
        displayName: "商品",
        summaryStatus: "failed",
        summaryError: "模型服务请求失败：离线",
        confirmedSource: "automatic",
        confirmedAt: "2026-08-13T00:00:00Z",
      }],
      summary: { total: 1, suggested: 0, conflict: 0, unmatched: 0, ineligible: 0, confirmed: 1 },
      total: 1,
    })
    mocks.retryMatchSummary.mockResolvedValue(undefined)
    render(<MatchingPage refreshToken="0:0" />)

    expect((await screen.findAllByText("商品")).length).toBeGreaterThanOrEqual(2)
    await user.click(screen.getByRole("button", { name: "重试商品摘要 INV-A" }))

    await waitFor(() => expect(mocks.retryMatchSummary).toHaveBeenCalledWith("invoice-a"))
  })
})
