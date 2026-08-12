import {
  AlertCircle,
  CheckCircle2,
  ChevronLeft,
  ChevronRight,
  CircleEllipsis,
  Clock3,
  FileClock,
  ListTodo,
} from "lucide-react"
import { useCallback, useEffect, useRef, useState } from "react"
import { toast } from "sonner"

import { BatchPanel } from "@/components/batch-panel"
import { InvoiceTasksTab } from "@/components/invoice-tasks-tab"
import { Button } from "@/components/ui/button"
import { Progress } from "@/components/ui/progress"
import { ScrollArea } from "@/components/ui/scroll-area"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { api, errorMessage } from "@/lib/api"
import { formatDateTime } from "@/lib/format"
import type { BatchDetail, ImportBatchesPage, ImportBatchSummary, InvoiceBatchDetail } from "@/lib/types"
import { cn } from "@/lib/utils"

interface ImportTasksPageProps {
  activeBatch: BatchDetail | null
  selectedBatchId: string | null
  onSelectedBatchChange: (id: string | null) => void
  onBatchChange: (batch: BatchDetail) => void
  activeInvoiceBatch: InvoiceBatchDetail | null
  selectedInvoiceBatchId: string | null
  onSelectedInvoiceBatchChange: (id: string | null) => void
  onInvoiceBatchChange: (batch: InvoiceBatchDetail) => void
}

const emptyPage: ImportBatchesPage = { items: [], total: 0, page: 1, pageSize: 20 }
const pageSizes = [20, 50, 100]

const batchStatusMeta: Record<BatchDetail["status"], {
  label: string
  icon: typeof Clock3
  className: string
}> = {
  queued: { label: "排队", icon: Clock3, className: "text-zinc-600" },
  processing: { label: "处理中", icon: CircleEllipsis, className: "text-blue-600" },
  completed: { label: "已完成", icon: CheckCircle2, className: "text-emerald-600" },
  completed_with_errors: { label: "部分失败", icon: AlertCircle, className: "text-amber-700" },
}

function progressOf(batch: ImportBatchSummary) {
  const finished = batch.counts.succeeded + batch.counts.failed + batch.counts.canceled +
    batch.counts.skipped + batch.counts.interrupted
  return batch.counts.total ? Math.round((finished / batch.counts.total) * 100) : 0
}

function summaryFromDetail(batch: BatchDetail): ImportBatchSummary {
  return {
    id: batch.id,
    platform: batch.platform,
    status: batch.status,
    createdAt: batch.createdAt,
    completedAt: batch.completedAt,
    counts: batch.counts,
  }
}

function BatchHistoryItem({ batch, selected, onSelect }: {
  batch: ImportBatchSummary
  selected: boolean
  onSelect: () => void
}) {
  const meta = batchStatusMeta[batch.status]
  const Icon = meta.icon
  const progress = progressOf(batch)

  return (
    <button
      type="button"
      className={cn(
        "grid w-full gap-3 border-b bg-white px-5 py-4 text-left transition-colors hover:bg-zinc-50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring",
        selected && "bg-zinc-100 hover:bg-zinc-100",
      )}
      onClick={onSelect}
    >
      <div className="flex min-w-0 items-center justify-between gap-3">
        <span className="truncate text-sm font-medium">{formatDateTime(batch.createdAt)}</span>
        <span className={cn("flex shrink-0 items-center gap-1.5 text-xs", meta.className)}>
          <Icon className={cn("size-3.5", batch.status === "processing" && "animate-pulse")} />
          {meta.label}
        </span>
      </div>
      <div className="flex items-center gap-3">
        <Progress value={progress} className="h-1.5" />
        <span className="w-9 shrink-0 text-right text-xs tabular-nums text-muted-foreground">
          {progress}%
        </span>
      </div>
      <div className="flex items-center justify-between text-xs text-muted-foreground">
        <span>共 {batch.counts.total} 张</span>
        <span>成功 {batch.counts.succeeded} · 失败 {batch.counts.failed}</span>
      </div>
    </button>
  )
}

export default function ImportTasksPage({
  activeBatch,
  selectedBatchId,
  onSelectedBatchChange,
  onBatchChange,
  activeInvoiceBatch,
  selectedInvoiceBatchId,
  onSelectedInvoiceBatchChange,
  onInvoiceBatchChange,
}: ImportTasksPageProps) {
  const [history, setHistory] = useState<ImportBatchesPage>(emptyPage)
  const [page, setPage] = useState(1)
  const [pageSize, setPageSize] = useState(20)
  const [historyLoading, setHistoryLoading] = useState(true)
  const [historyError, setHistoryError] = useState<string | null>(null)
  const [detail, setDetail] = useState<BatchDetail | null>(null)
  const [detailLoading, setDetailLoading] = useState(false)
  const [detailError, setDetailError] = useState<string | null>(null)
  const [historyVersion, setHistoryVersion] = useState(0)
  const [detailVersion, setDetailVersion] = useState(0)
  const selectedRef = useRef(selectedBatchId)
  const listRequestRef = useRef(0)
  const detailRequestRef = useRef(0)

  useEffect(() => {
    selectedRef.current = selectedBatchId
  }, [selectedBatchId])

  useEffect(() => {
    const request = ++listRequestRef.current
    setHistoryLoading(true)
    setHistoryError(null)
    api.listImportBatches({ page, pageSize })
      .then((result) => {
        if (request !== listRequestRef.current) return
        setHistory(result)
        const selected = selectedRef.current
        if (!selected || !result.items.some((batch) => batch.id === selected)) {
          onSelectedBatchChange(result.items[0]?.id ?? null)
        }
      })
      .catch((error) => {
        if (request !== listRequestRef.current) return
        const message = errorMessage(error)
        setHistory({ items: [], total: 0, page, pageSize })
        setHistoryError(message)
        onSelectedBatchChange(null)
        toast.error(message)
      })
      .finally(() => {
        if (request === listRequestRef.current) setHistoryLoading(false)
      })
  }, [historyVersion, onSelectedBatchChange, page, pageSize])

  useEffect(() => {
    const request = ++detailRequestRef.current
    if (!selectedBatchId) {
      setDetail(null)
      setDetailLoading(false)
      setDetailError(null)
      return
    }
    setDetail(null)
    setDetailLoading(true)
    setDetailError(null)
    api.getBatch(selectedBatchId)
      .then((batch) => {
        if (request === detailRequestRef.current) setDetail(batch)
      })
      .catch((error) => {
        if (request !== detailRequestRef.current) return
        const message = errorMessage(error)
        setDetailError(message)
        toast.error(message)
      })
      .finally(() => {
        if (request === detailRequestRef.current) setDetailLoading(false)
      })
  }, [detailVersion, selectedBatchId])

  useEffect(() => {
    if (activeBatch?.id !== selectedBatchId) return
    ++detailRequestRef.current
    setDetail(activeBatch)
    setDetailLoading(false)
    setDetailError(null)
  }, [activeBatch, selectedBatchId])

  useEffect(() => {
    if (!activeBatch) return
    setHistory((current) => ({
      ...current,
      items: current.items.map((batch) =>
        batch.id === activeBatch.id ? summaryFromDetail(activeBatch) : batch),
    }))
  }, [activeBatch])

  const handleBatchChange = useCallback((batch: BatchDetail) => {
    setDetail(batch)
    setHistory((current) => ({
      ...current,
      items: current.items.map((item) => item.id === batch.id ? summaryFromDetail(batch) : item),
    }))
    onBatchChange(batch)
  }, [onBatchChange])

  const pages = Math.max(1, Math.ceil(history.total / history.pageSize))

  return (
    <div className="flex h-screen min-h-0 flex-col bg-white">
      <header className="flex min-h-20 shrink-0 items-center border-b bg-white px-6">
        <div>
          <h1 className="text-lg font-semibold">导入任务</h1>
          <p className="mt-1 text-xs text-muted-foreground">查看和管理持久化的导入处理记录</p>
        </div>
      </header>
      <Tabs defaultValue="orders" className="min-h-0 flex-1 gap-0">
        <div className="flex h-12 shrink-0 items-end border-b bg-white px-6">
          <TabsList variant="line" className="h-11">
            <TabsTrigger value="orders" className="min-w-20">订单</TabsTrigger>
            <TabsTrigger value="invoices" className="min-w-20">发票</TabsTrigger>
          </TabsList>
        </div>
        <TabsContent value="orders" className="min-h-0 bg-zinc-100">
          <div className="grid h-full min-h-0 grid-rows-[minmax(220px,38%)_minmax(0,1fr)] xl:grid-cols-[minmax(300px,34%)_minmax(0,1fr)] xl:grid-rows-1">
            <section className="flex min-h-0 flex-col border-b bg-white xl:border-r xl:border-b-0" aria-label="导入批次">
              <div className="flex h-11 shrink-0 items-center justify-between border-b px-5">
                <span className="text-xs font-medium text-muted-foreground">历史批次</span>
                <span className="text-xs tabular-nums text-muted-foreground">{history.total} 条</span>
              </div>
              <ScrollArea className={cn("min-h-0 flex-1 bg-white", historyLoading && "opacity-60")}>
                {history.items.map((batch) => (
                  <BatchHistoryItem
                    key={batch.id}
                    batch={batch}
                    selected={batch.id === selectedBatchId}
                    onSelect={() => onSelectedBatchChange(batch.id)}
                  />
                ))}
                {!historyLoading && historyError && (
                  <div className="flex h-72 flex-col items-center justify-center gap-3 px-6 text-center text-muted-foreground">
                    <AlertCircle className="size-8 text-red-600" />
                    <p className="text-sm">导入历史加载失败</p>
                    <Button variant="outline" size="sm" onClick={() => setHistoryVersion((value) => value + 1)}>
                      重新加载
                    </Button>
                  </div>
                )}
                {!historyLoading && !historyError && history.items.length === 0 && (
                  <div className="flex h-72 flex-col items-center justify-center gap-3 text-muted-foreground">
                    <FileClock className="size-8" />
                    <p className="text-sm">暂无订单导入任务</p>
                  </div>
                )}
              </ScrollArea>
              <div className="flex h-14 shrink-0 items-center justify-between border-t bg-white px-4">
                <Select
                  value={String(pageSize)}
                  onValueChange={(value) => {
                    if (value === null) return
                    setPageSize(Number(value))
                    setPage(1)
                  }}
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
                <div className="flex items-center gap-2">
                  <span className="mr-1 text-xs tabular-nums text-muted-foreground">
                    {history.page} / {pages}
                  </span>
                  <Button
                    variant="outline"
                    size="icon-sm"
                    disabled={page <= 1 || historyLoading}
                    onClick={() => setPage((value) => value - 1)}
                    title="上一页"
                  >
                    <ChevronLeft />
                  </Button>
                  <Button
                    variant="outline"
                    size="icon-sm"
                    disabled={page >= pages || historyLoading}
                    onClick={() => setPage((value) => value + 1)}
                    title="下一页"
                  >
                    <ChevronRight />
                  </Button>
                </div>
              </div>
            </section>
            <section className="min-h-0 bg-white" aria-label="批次详情">
              {detail && <BatchPanel batch={detail} onBatchChange={handleBatchChange} />}
              {!detail && (
                <div className="flex h-full flex-col items-center justify-center gap-3 text-muted-foreground">
                  {detailError ? (
                    <AlertCircle className="size-8 text-red-600" />
                  ) : (
                    <ListTodo className={cn("size-8", detailLoading && "animate-pulse")} />
                  )}
                  <p className="text-sm">
                    {detailLoading ? "正在加载批次详情" : detailError ? "批次详情加载失败" : "请选择一个导入批次"}
                  </p>
                  {detailError && (
                    <Button variant="outline" size="sm" onClick={() => setDetailVersion((value) => value + 1)}>
                      重新加载
                    </Button>
                  )}
                </div>
              )}
            </section>
          </div>
        </TabsContent>
        <TabsContent value="invoices" className="min-h-0 bg-zinc-100">
          <InvoiceTasksTab
            activeBatch={activeInvoiceBatch}
            selectedBatchId={selectedInvoiceBatchId}
            onSelectedBatchChange={onSelectedInvoiceBatchChange}
            onBatchChange={onInvoiceBatchChange}
          />
        </TabsContent>
      </Tabs>
    </div>
  )
}
