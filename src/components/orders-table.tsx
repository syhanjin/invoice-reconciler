import {
  type ColumnDef,
  type ColumnSizingState,
  type RowSelectionState,
  type VisibilityState,
  flexRender,
  functionalUpdate,
  getCoreRowModel,
  useReactTable,
} from "@tanstack/react-table"
import {
  ArrowDown,
  ArrowUp,
  ArrowUpDown,
  ChevronsLeft,
  ChevronsRight,
  ChevronLeft,
  ChevronRight,
  Columns3Cog,
  PackageOpen,
  RotateCcw,
  Rows3,
  Trash2,
} from "lucide-react"
import { useEffect, useMemo, useState } from "react"

import { Badge } from "@/components/ui/badge"
import { LedgerStatusBadge, ledgerRowClass } from "@/components/ledger-status"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { Input } from "@/components/ui/input"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import { defaultOrderTablePreferences } from "@/lib/store"
import { formatDateTime, formatMoney } from "@/lib/format"
import type {
  OrderListItem,
  OrdersPage,
  OrderSortBy,
  OrderTableColumnId,
  OrderTablePreferences,
  SortDirection,
} from "@/lib/types"
import { cn } from "@/lib/utils"

interface OrdersTableProps {
  data: OrdersPage
  loading: boolean
  preferences: OrderTablePreferences
  selectedOrderIds: string[]
  onSelect: (id: string) => void
  onSelectionChange: (ids: string[]) => void
  onDeleteSelected: () => void
  onPageChange: (page: number) => void
  onPageSizeChange: (pageSize: number) => void
  onSortChange: (sortBy: OrderSortBy, sortDirection: SortDirection) => void
  onPreferencesChange: (preferences: OrderTablePreferences) => void
}

const pageSizes = [20, 50, 100]
const columnLabels: Partial<Record<OrderTableColumnId, string>> = {
  productSummary: "商品",
  shopName: "店铺",
  paidAmountFen: "实付",
  orderedAt: "下单时间",
  createdAt: "创建时间",
  platform: "来源",
  matchStatus: "匹配状态",
  projectName: "所属项目",
}

function freshDefaultPreferences(): OrderTablePreferences {
  return {
    ...defaultOrderTablePreferences,
    columnVisibility: {},
    columnSizing: {},
  }
}

interface SortableHeaderProps {
  label: string
  sortBy: OrderSortBy
  activeSortBy: OrderSortBy
  direction: SortDirection
  onChange: (sortBy: OrderSortBy, direction: SortDirection) => void
}

function SortableHeader({
  label,
  sortBy,
  activeSortBy,
  direction,
  onChange,
}: SortableHeaderProps) {
  const active = activeSortBy === sortBy
  const SortIcon = !active ? ArrowUpDown : direction === "asc" ? ArrowUp : ArrowDown

  return (
    <Button
      variant="ghost"
      size="sm"
      className="-ml-2 h-8 px-2 text-xs font-medium"
      onClick={(event) => {
        event.stopPropagation()
        if (!active) onChange(sortBy, "asc")
        else if (direction === "asc") onChange(sortBy, "desc")
        else if (sortBy === "created_at") onChange(sortBy, "asc")
        else onChange("created_at", "desc")
      }}
      aria-label={`${label}排序`}
    >
      {label}
      <SortIcon className={cn("size-3.5", !active && "text-muted-foreground")} />
    </Button>
  )
}

export function OrdersTable({
  data,
  loading,
  preferences,
  selectedOrderIds,
  onSelect,
  onSelectionChange,
  onDeleteSelected,
  onPageChange,
  onPageSizeChange,
  onSortChange,
  onPreferencesChange,
}: OrdersTableProps) {
  const selectedCurrentPageCount = data.items.filter((order) => selectedOrderIds.includes(order.id)).length
  const allCurrentPageSelected = data.items.length > 0 && selectedCurrentPageCount === data.items.length
  const someCurrentPageSelected = selectedCurrentPageCount > 0 && !allCurrentPageSelected
  const columns = useMemo<ColumnDef<OrderListItem>[]>(() => [
    {
      id: "select",
      header: () => (
        <div className="flex justify-center">
          <Checkbox
            checked={allCurrentPageSelected}
            indeterminate={someCurrentPageSelected}
            disabled={loading || data.items.length === 0}
            aria-label="选择当前页全部订单"
            onClick={(event) => event.stopPropagation()}
            onCheckedChange={(checked) => onSelectionChange(checked ? data.items.map((order) => order.id) : [])}
          />
        </div>
      ),
      cell: ({ row }) => (
        <div className="flex justify-center">
          <Checkbox
            checked={row.getIsSelected()}
            disabled={loading}
            aria-label={`选择订单 ${row.original.externalOrderNumber ?? row.original.id}`}
            onClick={(event) => event.stopPropagation()}
            onCheckedChange={(checked) => row.toggleSelected(checked)}
          />
        </div>
      ),
      size: 44,
      minSize: 44,
      maxSize: 44,
      enableHiding: false,
      enableResizing: false,
    },
    {
      accessorKey: "externalOrderNumber",
      header: "订单号",
      size: 210,
      minSize: 170,
      maxSize: 320,
      enableHiding: false,
      cell: ({ row }) => (
        <div className="min-w-0">
          <p className="truncate font-mono text-xs">
            {row.original.externalOrderNumber ?? "未识别"}
          </p>
          {row.original.possibleDuplicate && (
            <Badge variant="destructive" className="mt-1">可能重复</Badge>
          )}
        </div>
      ),
    },
    {
      accessorKey: "matchStatus",
      header: "匹配状态",
      size: 105,
      minSize: 90,
      maxSize: 130,
      cell: ({ row }) => <LedgerStatusBadge status={row.original.matchStatus} />,
    },
    {
      accessorKey: "projectName",
      header: "所属项目",
      size: 160,
      minSize: 120,
      maxSize: 240,
      cell: ({ row }) => row.original.projectName
        ? <Badge variant="secondary" className="max-w-full truncate">{row.original.projectName}</Badge>
        : <span className="text-xs text-muted-foreground">未分配</span>,
    },
    {
      accessorKey: "productSummary",
      header: "商品",
      size: 320,
      minSize: 220,
      maxSize: 520,
      cell: ({ row }) => (
        <div className="min-w-0">
          <p className="truncate text-sm">{row.original.productSummary || "-"}</p>
          {row.original.productCount > 2 && (
            <p className="text-xs text-muted-foreground">共 {row.original.productCount} 项</p>
          )}
        </div>
      ),
    },
    {
      accessorKey: "shopName",
      header: () => (
        <SortableHeader
          label="店铺"
          sortBy="shop_name"
          activeSortBy={preferences.sortBy}
          direction={preferences.sortDirection}
          onChange={onSortChange}
        />
      ),
      size: 170,
      minSize: 120,
      maxSize: 280,
      cell: ({ row }) => <span className="block truncate">{row.original.shopName ?? "-"}</span>,
    },
    {
      accessorKey: "paidAmountFen",
      header: () => (
        <div className="flex justify-end">
          <SortableHeader
            label="实付"
            sortBy="paid_amount"
            activeSortBy={preferences.sortBy}
            direction={preferences.sortDirection}
            onChange={onSortChange}
          />
        </div>
      ),
      size: 120,
      minSize: 100,
      maxSize: 180,
      cell: ({ row }) => (
        <div className="text-right font-medium tabular-nums">
          {formatMoney(row.original.paidAmountFen)}
        </div>
      ),
    },
    {
      accessorKey: "orderedAt",
      header: () => (
        <SortableHeader
          label="下单时间"
          sortBy="ordered_at"
          activeSortBy={preferences.sortBy}
          direction={preferences.sortDirection}
          onChange={onSortChange}
        />
      ),
      size: 180,
      minSize: 150,
      maxSize: 230,
      cell: ({ row }) => <span className="whitespace-nowrap">{formatDateTime(row.original.orderedAt)}</span>,
    },
    {
      accessorKey: "createdAt",
      header: () => (
        <SortableHeader
          label="创建时间"
          sortBy="created_at"
          activeSortBy={preferences.sortBy}
          direction={preferences.sortDirection}
          onChange={onSortChange}
        />
      ),
      size: 180,
      minSize: 150,
      maxSize: 230,
      cell: ({ row }) => <span className="whitespace-nowrap">{formatDateTime(row.original.createdAt)}</span>,
    },
    {
      accessorKey: "platform",
      header: "来源",
      size: 90,
      minSize: 80,
      maxSize: 130,
      cell: () => <Badge variant="outline" className="border-red-200 text-red-700">淘宝</Badge>,
    },
  ], [allCurrentPageSelected, data.items, loading, onSelectionChange, onSortChange, preferences.sortBy, preferences.sortDirection, someCurrentPageSelected])

  const updateVisibility = (updater: VisibilityState | ((old: VisibilityState) => VisibilityState)) => {
    const next = functionalUpdate(updater, preferences.columnVisibility as VisibilityState)
    delete next.externalOrderNumber
    onPreferencesChange({ ...preferences, columnVisibility: next })
  }
  const updateSizing = (updater: ColumnSizingState | ((old: ColumnSizingState) => ColumnSizingState)) => {
    const next = functionalUpdate(updater, preferences.columnSizing as ColumnSizingState)
    onPreferencesChange({ ...preferences, columnSizing: next })
  }

  const table = useReactTable({
    data: data.items,
    columns,
    getRowId: (row) => row.id,
    getCoreRowModel: getCoreRowModel(),
    manualPagination: true,
    manualSorting: true,
    rowCount: data.total,
    columnResizeMode: "onChange",
    state: {
      columnVisibility: preferences.columnVisibility,
      columnSizing: preferences.columnSizing,
      rowSelection: Object.fromEntries(selectedOrderIds.map((id) => [id, true])),
    },
    enableRowSelection: true,
    onColumnVisibilityChange: updateVisibility,
    onColumnSizingChange: updateSizing,
    onRowSelectionChange: (updater) => {
      const current = Object.fromEntries(selectedOrderIds.map((id) => [id, true])) as RowSelectionState
      const next = functionalUpdate(updater, current)
      onSelectionChange(Object.keys(next).filter((id) => next[id]))
    },
  })
  const pages = Math.max(1, Math.ceil(data.total / data.pageSize))
  const rangeStart = data.total === 0 ? 0 : (data.page - 1) * data.pageSize + 1
  const rangeEnd = Math.min(data.total, data.page * data.pageSize)
  const [pageInput, setPageInput] = useState(String(data.page))

  useEffect(() => setPageInput(String(data.page)), [data.page])

  const commitPageInput = () => {
    const requestedPage = Number(pageInput)
    if (!Number.isInteger(requestedPage)) {
      setPageInput(String(data.page))
      return
    }
    const nextPage = Math.min(pages, Math.max(1, requestedPage))
    setPageInput(String(nextPage))
    if (nextPage !== data.page) onPageChange(nextPage)
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col bg-white">
      <div className="flex h-11 shrink-0 items-center gap-2 overflow-x-auto border-b px-6">
        {selectedOrderIds.length > 0 && (
          <div className="flex shrink-0 items-center gap-2">
            <span className="text-xs font-medium tabular-nums">已选择 {selectedOrderIds.length} 项</span>
            <Button
              variant="ghost"
              size="sm"
              disabled={loading}
              onClick={() => onSelectionChange([])}
            >
              清除选择
            </Button>
            <Button
              variant="destructive"
              size="sm"
              disabled={loading}
              onClick={onDeleteSelected}
            >
              <Trash2 />
              删除
            </Button>
          </div>
        )}
        <div className="ml-auto flex shrink-0 items-center gap-2">
        <div className="flex h-8 items-center rounded-md border p-0.5" aria-label="表格密度">
          <Button
            variant={preferences.density === "comfortable" ? "secondary" : "ghost"}
            size="sm"
            className="h-6 rounded-sm px-2 text-xs"
            onClick={() => onPreferencesChange({ ...preferences, density: "comfortable" })}
          >
            舒适
          </Button>
          <Button
            variant={preferences.density === "compact" ? "secondary" : "ghost"}
            size="sm"
            className="h-6 rounded-sm px-2 text-xs"
            onClick={() => onPreferencesChange({ ...preferences, density: "compact" })}
          >
            紧凑
          </Button>
        </div>
        <DropdownMenu>
          <DropdownMenuTrigger
            render={
              <Button variant="outline" size="sm" aria-label="表格列设置">
                <Columns3Cog />
                列设置
              </Button>
            }
          />
          <DropdownMenuContent align="end" className="w-44">
            <DropdownMenuGroup>
              <DropdownMenuLabel>显示列</DropdownMenuLabel>
              {table.getAllLeafColumns().filter((column) => column.getCanHide()).map((column) => (
                <DropdownMenuCheckboxItem
                  key={column.id}
                  checked={column.getIsVisible()}
                  onCheckedChange={(checked) => column.toggleVisibility(checked)}
                >
                  {columnLabels[column.id as OrderTableColumnId]}
                </DropdownMenuCheckboxItem>
              ))}
            </DropdownMenuGroup>
            <DropdownMenuSeparator />
            <DropdownMenuItem onClick={() => onPreferencesChange(freshDefaultPreferences())}>
              <RotateCcw />
              恢复默认
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
        </div>
      </div>
      <div className="min-h-0 flex-1 overflow-auto">
        <Table className="table-fixed" style={{ minWidth: "100%", width: table.getTotalSize() }}>
          <TableHeader className="sticky top-0 z-10 bg-zinc-50">
            {table.getHeaderGroups().map((group) => (
              <TableRow key={group.id}>
                {group.headers.map((header) => (
                  <TableHead
                    key={header.id}
                    className={cn(
                      "relative h-10 overflow-hidden whitespace-nowrap",
                      header.column.id === "select" && "px-0",
                    )}
                    style={{ width: header.getSize() }}
                  >
                    {flexRender(header.column.columnDef.header, header.getContext())}
                    {header.column.getCanResize() && (
                      <button
                        type="button"
                        className={cn(
                          "absolute right-0 top-0 h-full w-1 cursor-col-resize touch-none select-none",
                          "hover:bg-zinc-400 focus-visible:bg-zinc-500",
                          header.column.getIsResizing() && "bg-zinc-500",
                        )}
                        aria-label={`调整${typeof header.column.columnDef.header === "string" ? header.column.columnDef.header : columnLabels[header.column.id as OrderTableColumnId] ?? "列"}宽度`}
                        onClick={(event) => event.stopPropagation()}
                        onMouseDown={(event) => {
                          event.stopPropagation()
                          header.getResizeHandler()(event)
                        }}
                        onTouchStart={(event) => {
                          event.stopPropagation()
                          header.getResizeHandler()(event)
                        }}
                      />
                    )}
                  </TableHead>
                ))}
              </TableRow>
            ))}
          </TableHeader>
          <TableBody className={loading ? "opacity-50" : undefined}>
            {table.getRowModel().rows.map((row) => (
              <TableRow
                key={row.id}
                data-state={row.getIsSelected() ? "selected" : undefined}
                className={cn(
                  "cursor-pointer",
                  ledgerRowClass(row.original.matchStatus, row.getIsSelected()),
                  preferences.density === "compact" ? "h-11" : "h-16",
                )}
                onClick={() => onSelect(row.original.id)}
              >
                {row.getVisibleCells().map((cell) => (
                  <TableCell
                    key={cell.id}
                    className={cn("overflow-hidden", cell.column.id === "select" && "px-0")}
                    style={{ width: cell.column.getSize() }}
                  >
                    {flexRender(cell.column.columnDef.cell, cell.getContext())}
                  </TableCell>
                ))}
              </TableRow>
            ))}
            {!loading && data.items.length === 0 && (
              <TableRow>
                <TableCell colSpan={table.getVisibleLeafColumns().length} className="h-72">
                  <div className="flex flex-col items-center gap-3 text-muted-foreground">
                    <PackageOpen className="size-8" />
                    <p className="text-sm">暂无订单</p>
                  </div>
                </TableCell>
              </TableRow>
            )}
          </TableBody>
        </Table>
      </div>
      <div className="flex h-14 shrink-0 items-center gap-4 overflow-x-auto border-t px-6">
        <p className="min-w-36 shrink-0 text-xs tabular-nums text-muted-foreground">
          {rangeStart}-{rangeEnd} / 共 {data.total} 条
        </p>
        <div className="ml-auto flex shrink-0 items-center gap-2">
          <Select
            value={String(preferences.pageSize)}
            onValueChange={(value) => value !== null && onPageSizeChange(Number(value))}
          >
            <SelectTrigger size="sm" className="w-20" aria-label="每页条数">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {pageSizes.map((size) => (
                <SelectItem key={size} value={String(size)}>{size} 条</SelectItem>
              ))}
            </SelectContent>
          </Select>
          <span className="mx-1 min-w-14 text-center text-xs tabular-nums text-muted-foreground">
            {data.page} / {pages}
          </span>
          <Button
            variant="outline"
            size="icon-sm"
            disabled={data.page <= 1 || loading}
            onClick={() => onPageChange(1)}
            title="首页"
          >
            <ChevronsLeft />
          </Button>
          <Button
            variant="outline"
            size="icon-sm"
            disabled={data.page <= 1 || loading}
            onClick={() => onPageChange(data.page - 1)}
            title="上一页"
          >
            <ChevronLeft />
          </Button>
          <form
            className="flex items-center gap-1"
            onSubmit={(event) => {
              event.preventDefault()
              commitPageInput()
            }}
          >
            <Rows3 className="size-3.5 text-muted-foreground" />
            <Input
              value={pageInput}
              onChange={(event) => setPageInput(event.target.value)}
              onBlur={commitPageInput}
              inputMode="numeric"
              aria-label="跳转页码"
              className="h-8 w-12 px-2 text-center text-xs tabular-nums"
              disabled={loading}
            />
          </form>
          <Button
            variant="outline"
            size="icon-sm"
            disabled={data.page >= pages || loading}
            onClick={() => onPageChange(data.page + 1)}
            title="下一页"
          >
            <ChevronRight />
          </Button>
          <Button
            variant="outline"
            size="icon-sm"
            disabled={data.page >= pages || loading}
            onClick={() => onPageChange(pages)}
            title="末页"
          >
            <ChevronsRight />
          </Button>
        </div>
      </div>
    </div>
  )
}
