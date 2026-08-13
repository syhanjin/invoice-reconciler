import { getCurrentWebview } from "@tauri-apps/api/webview"
import { open } from "@tauri-apps/plugin-dialog"
import { Filter, ImagePlus, LoaderCircle, Search, Trash2, X } from "lucide-react"
import { lazy, Suspense, useCallback, useEffect, useRef, useState } from "react"
import { toast } from "sonner"
import { useAtom } from "jotai"

import { OrdersTable } from "@/components/orders-table"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
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
import { orderTablePreferencesAtom } from "@/lib/store"
import type {
  LedgerMatchStatus,
  OrdersPage as OrdersPageData,
  OrderSortBy,
  ProjectMembershipStatus,
  ProjectSummary,
  SortDirection,
} from "@/lib/types"
import { cn } from "@/lib/utils"

interface OrdersPageProps {
  onImagesImported: () => void
  refreshToken: number
  workspaceRefreshToken?: number
}

const emptyPage: OrdersPageData = { items: [], total: 0, page: 1, pageSize: 20 }
const OrderSheet = lazy(() => import("@/components/order-sheet"))

export function OrdersPage({ onImagesImported, refreshToken, workspaceRefreshToken = 0 }: OrdersPageProps) {
  const [orders, setOrders] = useState<OrdersPageData>(emptyPage)
  const [preferences, setPreferences] = useAtom(orderTablePreferencesAtom)
  const [page, setPage] = useState(1)
  const [search, setSearch] = useState("")
  const [committedSearch, setCommittedSearch] = useState("")
  const [duplicateOnly, setDuplicateOnly] = useState(false)
  const [matchStatus, setMatchStatus] = useState("all")
  const [projectStatus, setProjectStatus] = useState("all")
  const [projectId, setProjectId] = useState("all")
  const [projects, setProjects] = useState<ProjectSummary[]>([])
  const [loading, setLoading] = useState(true)
  const [importing, setImporting] = useState(false)
  const [dragging, setDragging] = useState(false)
  const [selectedOrder, setSelectedOrder] = useState<string | null>(null)
  const [selectedOrderIds, setSelectedOrderIds] = useState<string[]>([])
  const [bulkDeleteOpen, setBulkDeleteOpen] = useState(false)
  const [bulkDeleting, setBulkDeleting] = useState(false)
  const [localRefreshToken, setLocalRefreshToken] = useState(0)
  const refreshRef = useRef(0)

  const clearSelection = useCallback(() => {
    setSelectedOrderIds([])
    setBulkDeleteOpen(false)
  }, [])

  const loadOrders = useCallback(async () => {
    const request = ++refreshRef.current
    setLoading(true)
    try {
      const result = await api.listOrders({
        page,
        pageSize: preferences.pageSize,
        search: committedSearch || undefined,
        duplicateOnly,
        matchStatus: matchStatus === "all" ? undefined : matchStatus as LedgerMatchStatus,
        projectStatus: projectStatus === "all" ? undefined : projectStatus as ProjectMembershipStatus,
        projectId: projectId === "all" ? undefined : projectId,
        sortBy: preferences.sortBy,
        sortDirection: preferences.sortDirection,
      })
      if (request === refreshRef.current) {
        const pages = Math.max(1, Math.ceil(result.total / result.pageSize))
        if (page > pages) {
          clearSelection()
          setPage(pages)
        } else {
          setOrders(result)
          const visibleIds = new Set(result.items.map((order) => order.id))
          setSelectedOrderIds((current) => current.filter((id) => visibleIds.has(id)))
        }
      }
    } catch (error) {
      toast.error(errorMessage(error))
    } finally {
      if (request === refreshRef.current) setLoading(false)
    }
  }, [clearSelection, committedSearch, duplicateOnly, matchStatus, page, preferences.pageSize, preferences.sortBy, preferences.sortDirection, projectId, projectStatus])

  useEffect(() => void loadOrders(), [loadOrders, localRefreshToken, refreshToken, workspaceRefreshToken])
  useEffect(() => {
    api.listProjects({ page: 1, pageSize: 100, archived: false })
      .then((result) => setProjects(result.items))
      .catch((error) => toast.error(errorMessage(error)))
  }, [workspaceRefreshToken])

  useEffect(() => {
    if (!selectedOrderIds.length && !bulkDeleting) setBulkDeleteOpen(false)
  }, [bulkDeleting, selectedOrderIds.length])

  const deleteSelectedOrders = async () => {
    if (!selectedOrderIds.length || bulkDeleting) return
    const requestedIds = [...selectedOrderIds]
    setBulkDeleting(true)
    try {
      const result = await api.deleteOrders(requestedIds)
      const deletedIds = new Set(result.results.filter((item) => item.deleted).map((item) => item.id))
      const failures = result.results.filter((item) => !item.deleted)
      setSelectedOrderIds((current) => current.filter((id) => !deletedIds.has(id)))
      setBulkDeleteOpen(false)

      if (deletedIds.size > 0) toast.success(`已删除 ${deletedIds.size} 个订单`)
      if (failures.length > 0) {
        const orderLabels = new Map(orders.items.map((order) => [order.id, order.externalOrderNumber ?? order.id]))
        const failedLabels = failures.slice(0, 3).map((item) => orderLabels.get(item.id) ?? item.id)
        const remaining = failures.length - failedLabels.length
        toast.error(`${failures.length} 个订单删除失败：${failedLabels.join("、")}${remaining > 0 ? ` 等 ${failures.length} 个` : ""}`)
      }
      setLocalRefreshToken((current) => current + 1)
    } catch (error) {
      toast.error(errorMessage(error), { duration: 6000 })
    } finally {
      setBulkDeleting(false)
    }
  }

  const importPaths = useCallback(
    async (paths: string[]) => {
      if (!paths.length || importing) return
      setImporting(true)
      try {
        const result = await api.importOrderImages(paths)
        onImagesImported()
        toast.success(`已加入 ${result.imported} 张图片${result.skipped ? `，跳过 ${result.skipped} 张重复图片` : ""}`)
      } catch (error) {
        toast.error(errorMessage(error), { duration: 6000 })
      } finally {
        setImporting(false)
      }
    },
    [importing, onImagesImported],
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
            clearSelection()
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
                clearSelection()
                setSearch("")
                setCommittedSearch("")
                setPage(1)
              }}
            >
              <X />
            </Button>
          )}
        </form>
        <Select value={matchStatus} onValueChange={(value) => { clearSelection(); setMatchStatus(value ?? "all"); setPage(1) }}>
          <SelectTrigger aria-label="匹配状态筛选" className="w-28"><SelectValue /></SelectTrigger>
          <SelectContent><SelectItem value="all">全部匹配</SelectItem><SelectItem value="unmatched">未匹配</SelectItem><SelectItem value="matched">已匹配</SelectItem><SelectItem value="confirmed">已确认</SelectItem></SelectContent>
        </Select>
        <Select value={projectStatus} onValueChange={(value) => { const next = value ?? "all"; clearSelection(); setProjectStatus(next); if (next !== "assigned") setProjectId("all"); setPage(1) }}>
          <SelectTrigger aria-label="项目状态筛选" className="w-28"><SelectValue /></SelectTrigger>
          <SelectContent><SelectItem value="all">全部归属</SelectItem><SelectItem value="unassigned">未分配</SelectItem><SelectItem value="assigned">已分配</SelectItem></SelectContent>
        </Select>
        <Select value={projectId} onValueChange={(value) => { const next = value ?? "all"; clearSelection(); setProjectId(next); setProjectStatus(next === "all" ? "all" : "assigned"); setPage(1) }}>
          <SelectTrigger aria-label="具体项目筛选" className="w-36"><SelectValue /></SelectTrigger>
          <SelectContent><SelectItem value="all">全部项目</SelectItem>{projects.map((project) => <SelectItem key={project.id} value={project.id}>{project.name}</SelectItem>)}</SelectContent>
        </Select>
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
              clearSelection()
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
        preferences={preferences}
        selectedOrderIds={selectedOrderIds}
        onSelect={setSelectedOrder}
        onSelectionChange={setSelectedOrderIds}
        onDeleteSelected={() => setBulkDeleteOpen(true)}
        onPageChange={(nextPage) => {
          clearSelection()
          setPage(nextPage)
        }}
        onPageSizeChange={(pageSize) => {
          clearSelection()
          setPreferences((current) => ({ ...current, pageSize }))
          setPage(1)
        }}
        onSortChange={(sortBy: OrderSortBy, sortDirection: SortDirection) => {
          clearSelection()
          setPreferences((current) => ({ ...current, sortBy, sortDirection }))
          setPage(1)
        }}
        onPreferencesChange={(nextPreferences) => {
          const queryChanged = nextPreferences.pageSize !== preferences.pageSize
            || nextPreferences.sortBy !== preferences.sortBy
            || nextPreferences.sortDirection !== preferences.sortDirection
          setPreferences(nextPreferences)
          if (queryChanged) {
            clearSelection()
            setPage(1)
          }
        }}
      />
      <Dialog
        open={bulkDeleteOpen}
        onOpenChange={(open) => {
          if (!bulkDeleting) setBulkDeleteOpen(open)
        }}
      >
        <DialogContent showCloseButton={!bulkDeleting}>
          <DialogHeader>
            <DialogTitle>删除选中的订单？</DialogTitle>
            <DialogDescription>
              将删除 {selectedOrderIds.length} 个订单及其商品记录和原始截图。此操作无法撤销。
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              variant="outline"
              disabled={bulkDeleting}
              onClick={() => setBulkDeleteOpen(false)}
            >
              取消
            </Button>
            <Button
              variant="destructive"
              disabled={bulkDeleting}
              onClick={() => void deleteSelectedOrders()}
            >
              {bulkDeleting ? <LoaderCircle className="animate-spin" /> : <Trash2 />}
              确认删除
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
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
