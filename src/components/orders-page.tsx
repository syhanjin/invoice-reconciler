import { getCurrentWebview } from "@tauri-apps/api/webview"
import { open } from "@tauri-apps/plugin-dialog"
import { Filter, ImagePlus, LoaderCircle, Search, X } from "lucide-react"
import { lazy, Suspense, useCallback, useEffect, useRef, useState } from "react"
import { toast } from "sonner"

import { OrdersTable } from "@/components/orders-table"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { api, errorMessage } from "@/lib/api"
import type { BatchDetail, OrdersPage as OrdersPageData } from "@/lib/types"
import { cn } from "@/lib/utils"

interface OrdersPageProps {
  onBatchCreated: (batch: BatchDetail) => void
  refreshToken: number
}

const emptyPage: OrdersPageData = { items: [], total: 0, page: 1, pageSize: 20 }
const OrderSheet = lazy(() => import("@/components/order-sheet"))

export function OrdersPage({ onBatchCreated, refreshToken }: OrdersPageProps) {
  const [orders, setOrders] = useState<OrdersPageData>(emptyPage)
  const [page, setPage] = useState(1)
  const [search, setSearch] = useState("")
  const [committedSearch, setCommittedSearch] = useState("")
  const [duplicateOnly, setDuplicateOnly] = useState(false)
  const [loading, setLoading] = useState(true)
  const [importing, setImporting] = useState(false)
  const [dragging, setDragging] = useState(false)
  const [selectedOrder, setSelectedOrder] = useState<string | null>(null)
  const refreshRef = useRef(0)

  const loadOrders = useCallback(async () => {
    const request = ++refreshRef.current
    setLoading(true)
    try {
      const result = await api.listOrders({
        page,
        pageSize: 20,
        search: committedSearch || undefined,
        duplicateOnly,
        sortBy: "created_at",
        sortDirection: "desc",
      })
      if (request === refreshRef.current) setOrders(result)
    } catch (error) {
      toast.error(errorMessage(error))
    } finally {
      if (request === refreshRef.current) setLoading(false)
    }
  }, [committedSearch, duplicateOnly, page])

  useEffect(() => void loadOrders(), [loadOrders, refreshToken])

  const importPaths = useCallback(
    async (paths: string[]) => {
      if (!paths.length || importing) return
      setImporting(true)
      try {
        const batch = await api.createImportBatch(paths)
        onBatchCreated(batch)
        toast.success(`已提交 ${paths.length} 张图片`)
      } catch (error) {
        toast.error(errorMessage(error), { duration: 6000 })
      } finally {
        setImporting(false)
      }
    },
    [importing, onBatchCreated],
  )

  useEffect(() => {
    let disposed = false
    let unlisten: (() => void) | undefined
    getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type === "enter" || event.payload.type === "over") setDragging(true)
        if (event.payload.type === "leave") setDragging(false)
        if (event.payload.type === "drop") {
          setDragging(false)
          void importPaths(event.payload.paths)
        }
      })
      .then((dispose) => {
        if (disposed) dispose()
        else unlisten = dispose
      })
      .catch(() => undefined)
    return () => {
      disposed = true
      unlisten?.()
    }
  }, [importPaths])

  return (
    <div className="relative flex h-screen min-h-0 flex-col">
      {dragging && (
        <div className="pointer-events-none absolute inset-3 z-50 flex items-center justify-center rounded-md border-2 border-dashed border-zinc-500 bg-white/90 shadow-lg">
          <div className="text-center">
            <ImagePlus className="mx-auto mb-3 size-8" />
            <p className="text-sm font-medium">释放以导入订单截图</p>
          </div>
        </div>
      )}
      <header className="flex min-h-20 shrink-0 items-center justify-between border-b bg-white px-6">
        <div>
          <h1 className="text-lg font-semibold">订单</h1>
          <p className="mt-1 text-xs text-muted-foreground">从截图识别并维护订单数据</p>
        </div>
        <div className="flex items-center gap-2">
          <Select value="taobao">
            <SelectTrigger className="w-28">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="taobao">淘宝</SelectItem>
            </SelectContent>
          </Select>
          <Button
            disabled={importing}
            onClick={async () => {
              const selected = await open({
                multiple: true,
                directory: false,
                title: "选择淘宝订单截图",
                filters: [
                  {
                    name: "订单截图",
                    extensions: ["bmp", "gif", "jpeg", "jpg", "png", "tif", "tiff", "webp"],
                  },
                ],
              })
              if (selected) await importPaths(Array.isArray(selected) ? selected : [selected])
            }}
          >
            {importing ? <LoaderCircle className="animate-spin" /> : <ImagePlus />}
            导入截图
          </Button>
        </div>
      </header>
      <div className="flex min-h-14 shrink-0 items-center gap-3 border-b bg-white px-6">
        <form
          className="relative w-full max-w-sm"
          onSubmit={(event) => {
            event.preventDefault()
            setPage(1)
            setCommittedSearch(search.trim())
          }}
        >
          <Search className="absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
          <Input
            value={search}
            onChange={(event) => setSearch(event.target.value)}
            className="pl-9 pr-9"
            placeholder="搜索订单号、商品或店铺"
          />
          {search && (
            <Button
              type="button"
              variant="ghost"
              size="icon-sm"
              className="absolute right-1 top-1/2 -translate-y-1/2"
              onClick={() => {
                setSearch("")
                setCommittedSearch("")
                setPage(1)
              }}
            >
              <X />
            </Button>
          )}
        </form>
        <div
          className={cn(
            "flex h-9 items-center gap-2 rounded-md border px-3",
            duplicateOnly && "border-red-200 bg-red-50",
          )}
        >
          <Filter className="size-3.5 text-muted-foreground" />
          <Checkbox
            id="duplicate-only"
            checked={duplicateOnly}
            onCheckedChange={(checked) => {
              setDuplicateOnly(checked === true)
              setPage(1)
            }}
          />
          <Label htmlFor="duplicate-only" className="cursor-pointer text-xs font-normal">
            仅可能重复
          </Label>
        </div>
      </div>
      <OrdersTable
        data={orders}
        loading={loading}
        onSelect={setSelectedOrder}
        onPageChange={setPage}
      />
      <Suspense fallback={null}>
        <OrderSheet
          orderId={selectedOrder}
          onOpenChange={(open) => !open && setSelectedOrder(null)}
          onChanged={() => {
            void loadOrders()
          }}
        />
      </Suspense>
    </div>
  )
}
