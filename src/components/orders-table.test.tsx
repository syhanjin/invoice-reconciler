import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { useState } from "react"
import { afterEach, describe, expect, it, vi } from "vitest"

import { defaultOrderTablePreferences } from "@/lib/store"
import type { OrdersPage, OrderTablePreferences } from "@/lib/types"

import { OrdersTable } from "./orders-table"

const page: OrdersPage = {
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
  total: 95,
  page: 2,
  pageSize: 20,
}

function Harness({
  onSelect = vi.fn(),
  onPageChange = vi.fn(),
  onPageSizeChange = vi.fn(),
  onSortChange = vi.fn(),
}: {
  onSelect?: (id: string) => void
  onPageChange?: (page: number) => void
  onPageSizeChange?: (pageSize: number) => void
  onSortChange?: (sortBy: "created_at" | "ordered_at" | "paid_amount" | "shop_name", direction: "asc" | "desc") => void
}) {
  const [preferences, setPreferences] = useState<OrderTablePreferences>(defaultOrderTablePreferences)
  return (
    <OrdersTable
      data={page}
      loading={false}
      preferences={preferences}
      onSelect={onSelect}
      onPageChange={onPageChange}
      onPageSizeChange={onPageSizeChange}
      onSortChange={onSortChange}
      onPreferencesChange={setPreferences}
    />
  )
}

describe("OrdersTable", () => {
  afterEach(cleanup)

  it("provides full pagination and clamps page-number input", async () => {
    const user = userEvent.setup()
    const onPageChange = vi.fn()
    const onPageSizeChange = vi.fn()
    render(<Harness onPageChange={onPageChange} onPageSizeChange={onPageSizeChange} />)

    expect(screen.getByText("21-40 / 共 95 条")).toBeInTheDocument()
    await user.click(screen.getByTitle("首页"))
    await user.click(screen.getByTitle("上一页"))
    await user.click(screen.getByTitle("下一页"))
    await user.click(screen.getByTitle("末页"))
    expect(onPageChange.mock.calls.map(([value]) => value)).toEqual([1, 1, 3, 5])

    const pageInput = screen.getByRole("textbox", { name: "跳转页码" })
    await user.clear(pageInput)
    await user.type(pageInput, "99{Enter}")
    expect(onPageChange).toHaveBeenLastCalledWith(5)
    await user.clear(pageInput)
    await user.type(pageInput, "abc{Enter}")
    expect(pageInput).toHaveValue("2")

    await user.click(screen.getByRole("combobox", { name: "每页条数" }))
    await user.click(await screen.findByRole("option", { name: "50 条" }))
    expect(onPageSizeChange).toHaveBeenCalledWith(50)
  })

  it("changes density, hides optional columns, and restores defaults", async () => {
    const user = userEvent.setup()
    render(<Harness />)

    await user.click(screen.getByRole("button", { name: "紧凑" }))
    expect(screen.getByRole("row", { name: /10001/ })).toHaveClass("h-11")

    await user.click(screen.getByRole("button", { name: "表格列设置" }))
    await user.click(await screen.findByRole("menuitemcheckbox", { name: "商品" }))
    expect(screen.queryByRole("columnheader", { name: "商品" })).not.toBeInTheDocument()
    expect(screen.getByRole("columnheader", { name: "订单号" })).toBeInTheDocument()

    await user.click(await screen.findByRole("menuitem", { name: "恢复默认" }))
    expect(screen.getByRole("columnheader", { name: "商品" })).toBeInTheDocument()
    expect(screen.getByRole("row", { name: /10001/ })).toHaveClass("h-16")
  })

  it("sorts through the controlled callback and keeps resize clicks out of rows", async () => {
    const user = userEvent.setup()
    const onSortChange = vi.fn()
    const onSelect = vi.fn()
    render(<Harness onSortChange={onSortChange} onSelect={onSelect} />)

    for (const [label, field] of [
      ["店铺排序", "shop_name"],
      ["实付排序", "paid_amount"],
      ["下单时间排序", "ordered_at"],
    ] as const) {
      await user.click(screen.getByRole("button", { name: label }))
      expect(onSortChange).toHaveBeenLastCalledWith(field, "asc")
    }
    await user.click(screen.getByRole("button", { name: "创建时间排序" }))
    expect(onSortChange).toHaveBeenLastCalledWith("created_at", "asc")

    fireEvent.click(screen.getByRole("button", { name: "调整订单号宽度" }))
    expect(onSelect).not.toHaveBeenCalled()
    await user.click(screen.getByRole("row", { name: /10001/ }))
    expect(onSelect).toHaveBeenCalledWith("order-1")
  })

  it("cycles an active sortable header through descending and the default sort", async () => {
    const user = userEvent.setup()
    const onSortChange = vi.fn()

    function SortHarness() {
      const [preferences, setPreferences] = useState<OrderTablePreferences>({
        ...defaultOrderTablePreferences,
        sortBy: "shop_name",
        sortDirection: "asc",
      })
      return (
        <OrdersTable
          data={page}
          loading={false}
          preferences={preferences}
          onSelect={vi.fn()}
          onPageChange={vi.fn()}
          onPageSizeChange={vi.fn()}
          onSortChange={(sortBy, sortDirection) => {
            onSortChange(sortBy, sortDirection)
            setPreferences((current) => ({ ...current, sortBy, sortDirection }))
          }}
          onPreferencesChange={setPreferences}
        />
      )
    }

    render(<SortHarness />)
    await user.click(screen.getByRole("button", { name: "店铺排序" }))
    expect(onSortChange).toHaveBeenLastCalledWith("shop_name", "desc")
    await user.click(screen.getByRole("button", { name: "店铺排序" }))
    expect(onSortChange).toHaveBeenLastCalledWith("created_at", "desc")
  })
})
