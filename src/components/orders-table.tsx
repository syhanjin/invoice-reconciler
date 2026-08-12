import {
  type ColumnDef,
  flexRender,
  getCoreRowModel,
  useReactTable,
} from "@tanstack/react-table"
import { ChevronLeft, ChevronRight, PackageOpen } from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import { formatDateTime, formatMoney } from "@/lib/format"
import type { OrderListItem, OrdersPage } from "@/lib/types"

interface OrdersTableProps {
  data: OrdersPage
  loading: boolean
  onSelect: (id: string) => void
  onPageChange: (page: number) => void
}

const columns: ColumnDef<OrderListItem>[] = [
  {
    accessorKey: "externalOrderNumber",
    header: "订单号",
    cell: ({ row }) => (
      <div className="max-w-48">
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
    accessorKey: "productSummary",
    header: "商品",
    cell: ({ row }) => (
      <div className="max-w-[360px]">
        <p className="truncate text-sm">{row.original.productSummary || "-"}</p>
        {row.original.productCount > 2 && (
          <p className="text-xs text-muted-foreground">共 {row.original.productCount} 项</p>
        )}
      </div>
    ),
  },
  {
    accessorKey: "shopName",
    header: "店铺",
    cell: ({ row }) => <span className="block max-w-40 truncate">{row.original.shopName ?? "-"}</span>,
  },
  {
    accessorKey: "paidAmountFen",
    header: () => <div className="text-right">实付</div>,
    cell: ({ row }) => (
      <div className="text-right font-medium tabular-nums">
        {formatMoney(row.original.paidAmountFen)}
      </div>
    ),
  },
  {
    accessorKey: "orderedAt",
    header: "下单时间",
    cell: ({ row }) => <span className="whitespace-nowrap">{formatDateTime(row.original.orderedAt)}</span>,
  },
  {
    accessorKey: "platform",
    header: "来源",
    cell: () => <Badge variant="outline" className="border-red-200 text-red-700">淘宝</Badge>,
  },
]

export function OrdersTable({ data, loading, onSelect, onPageChange }: OrdersTableProps) {
  const table = useReactTable({
    data: data.items,
    columns,
    getCoreRowModel: getCoreRowModel(),
    manualPagination: true,
    rowCount: data.total,
  })
  const pages = Math.max(1, Math.ceil(data.total / data.pageSize))

  return (
    <div className="flex min-h-0 flex-1 flex-col bg-white">
      <div className="min-h-0 flex-1 overflow-auto">
        <Table>
          <TableHeader className="sticky top-0 z-10 bg-zinc-50">
            {table.getHeaderGroups().map((group) => (
              <TableRow key={group.id}>
                {group.headers.map((header) => (
                  <TableHead key={header.id} className="h-10 whitespace-nowrap">
                    {flexRender(header.column.columnDef.header, header.getContext())}
                  </TableHead>
                ))}
              </TableRow>
            ))}
          </TableHeader>
          <TableBody className={loading ? "opacity-50" : undefined}>
            {table.getRowModel().rows.map((row) => (
              <TableRow
                key={row.id}
                className="h-16 cursor-pointer"
                onClick={() => onSelect(row.original.id)}
              >
                {row.getVisibleCells().map((cell) => (
                  <TableCell key={cell.id}>
                    {flexRender(cell.column.columnDef.cell, cell.getContext())}
                  </TableCell>
                ))}
              </TableRow>
            ))}
            {!loading && data.items.length === 0 && (
              <TableRow>
                <TableCell colSpan={columns.length} className="h-72">
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
      <div className="flex h-14 shrink-0 items-center justify-between border-t px-6">
        <p className="text-xs text-muted-foreground">共 {data.total} 条订单</p>
        <div className="flex items-center gap-2">
          <span className="mr-2 text-xs tabular-nums text-muted-foreground">
            {data.page} / {pages}
          </span>
          <Button
            variant="outline"
            size="icon-sm"
            disabled={data.page <= 1 || loading}
            onClick={() => onPageChange(data.page - 1)}
            title="上一页"
          >
            <ChevronLeft />
          </Button>
          <Button
            variant="outline"
            size="icon-sm"
            disabled={data.page >= pages || loading}
            onClick={() => onPageChange(data.page + 1)}
            title="下一页"
          >
            <ChevronRight />
          </Button>
        </div>
      </div>
    </div>
  )
}
