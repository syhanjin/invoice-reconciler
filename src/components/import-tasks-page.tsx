import {
  AlertCircle,
  Ban,
  CheckCircle2,
  ChevronLeft,
  ChevronRight,
  Clock3,
  CloudUpload,
  FileClock,
  LoaderCircle,
  RefreshCw,
  RotateCcw,
  Trash2,
} from "lucide-react"
import { useCallback, useEffect, useState } from "react"
import { toast } from "sonner"

import { InvoiceTasksTab } from "@/components/invoice-tasks-tab"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { api, errorMessage } from "@/lib/api"
import { formatDateTime } from "@/lib/format"
import type {
  AppSettings,
  InvoiceBatchDetail,
  OrderImportLog,
  OrderImportLogsPage,
  OrderImportPool,
  OrderImportTask,
} from "@/lib/types"
import { cn } from "@/lib/utils"

interface ImportTasksPageProps {
  settings: AppSettings
  orderRefreshToken: number
  activeInvoiceBatch: InvoiceBatchDetail | null
  selectedInvoiceBatchId: string | null
  onSelectedInvoiceBatchChange: (id: string | null) => void
  onInvoiceBatchChange: (batch: InvoiceBatchDetail) => void
}

const emptyPool: OrderImportPool = { tasks: [], batches: [], pending: 0, processing: 0, failed: 0 }
const emptyLogs: OrderImportLogsPage = { items: [], total: 0, page: 1, pageSize: 20 }
const pageSizes = [20, 50, 100]

const taskStatus = {
  pending: { label: "待处理", icon: Clock3, className: "text-zinc-600" },
  processing: { label: "识别中", icon: LoaderCircle, className: "text-blue-600" },
  batch_submitting: { label: "提交中", icon: CloudUpload, className: "text-blue-600" },
  batch_locked: { label: "批次处理中", icon: LoaderCircle, className: "text-blue-600" },
  failed: { label: "等待重试", icon: AlertCircle, className: "text-red-600" },
} as const

export default function ImportTasksPage({
  settings,
  orderRefreshToken,
  activeInvoiceBatch,
  selectedInvoiceBatchId,
  onSelectedInvoiceBatchChange,
  onInvoiceBatchChange,
}: ImportTasksPageProps) {
  return (
    <div className="flex h-screen min-h-0 flex-col bg-white">
      <header className="flex min-h-20 shrink-0 items-center border-b bg-white px-6">
        <div>
          <h1 className="text-lg font-semibold">导入任务</h1>
          <p className="mt-1 text-xs text-muted-foreground">管理订单识别池与发票导入记录</p>
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
          <OrderPoolWorkspace settings={settings} refreshToken={orderRefreshToken} />
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

function OrderPoolWorkspace({ settings, refreshToken }: { settings: AppSettings; refreshToken: number }) {
  const [pool, setPool] = useState<OrderImportPool>(emptyPool)
  const [logs, setLogs] = useState<OrderImportLogsPage>(emptyLogs)
  const [page, setPage] = useState(1)
  const [pageSize, setPageSize] = useState(20)
  const [loading, setLoading] = useState(true)
  const [pending, setPending] = useState<string | null>(null)
  const [deleteTarget, setDeleteTarget] = useState<OrderImportTask | null>(null)
  const [version, setVersion] = useState(0)

  const reloadPool = useCallback(async () => {
    try {
      setPool(await api.getOrderImportPool())
    } catch (error) {
      toast.error(errorMessage(error))
    }
  }, [])

  const reloadLogs = useCallback(async () => {
    try {
      const result = await api.listOrderImportLogs({ page, pageSize })
      const pages = Math.max(1, Math.ceil(result.total / result.pageSize))
      if (page > pages) setPage(pages)
      else setLogs(result)
    } catch (error) {
      toast.error(errorMessage(error))
    }
  }, [page, pageSize])

  useEffect(() => {
    setLoading(true)
    Promise.all([reloadPool(), reloadLogs()]).finally(() => setLoading(false))
  }, [refreshToken, version, reloadLogs, reloadPool])

  const run = async (key: string, action: () => Promise<unknown>, success?: string) => {
    setPending(key)
    try {
      await action()
      if (success) toast.success(success)
      setVersion((value) => value + 1)
      return true
    } catch (error) {
      toast.error(errorMessage(error), { duration: 6000 })
      return false
    } finally {
      setPending(null)
    }
  }

  const poolPanel = (
    <PoolPanel
      pool={pool}
      provider={settings.orderRecognitionProvider}
      disabled={pending !== null}
      pendingKey={pending}
      onRetry={(id) => run(id, () => api.retryOrderImportTask(id), "任务已重新排队")}
      onDelete={(id) => setDeleteTarget(pool.tasks.find((task) => task.id === id) ?? null)}
      onRetryAll={() => run("retry-all", () => api.retryFailedOrderImportTasks(), "失败任务已重新排队")}
      onSubmit={() => run("submit", () => api.submitOrderRecognitionBatch(), "批次正在提交")}
      onCancelBatch={(id) => run(`cancel:${id}`, () => api.cancelOrderRecognitionBatch(id), "已请求取消批次")}
    />
  )
  const logsPanel = (
    <LogsPanel
      logs={logs}
      page={page}
      pageSize={pageSize}
      loading={loading}
      onPageChange={setPage}
      onPageSizeChange={(value) => { setPageSize(value); setPage(1) }}
    />
  )

  return (
    <>
      <div className={cn("hidden h-full min-h-0 grid-cols-[minmax(0,1.15fr)_minmax(360px,.85fr)] divide-x xl:grid", loading && "opacity-80")}>
        {poolPanel}
        {logsPanel}
      </div>
      <Tabs defaultValue="pool" className="h-full min-h-0 gap-0 xl:hidden">
        <div className="flex h-11 items-end border-b bg-white px-4">
          <TabsList variant="line">
            <TabsTrigger value="pool">任务池 {pool.tasks.length}</TabsTrigger>
            <TabsTrigger value="logs">导入完成 {logs.total}</TabsTrigger>
          </TabsList>
        </div>
        <TabsContent value="pool" className="min-h-0">{poolPanel}</TabsContent>
        <TabsContent value="logs" className="min-h-0">{logsPanel}</TabsContent>
      </Tabs>
      <Dialog open={deleteTarget !== null} onOpenChange={(open) => !open && pending === null && setDeleteTarget(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>删除识别任务？</DialogTitle>
            <DialogDescription>
              {deleteTarget?.originalFileName} 将从任务池移除，同时删除已管理的图片。此操作无法撤销。
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" disabled={pending !== null} onClick={() => setDeleteTarget(null)}>取消</Button>
            <Button
              variant="destructive"
              disabled={pending !== null || deleteTarget === null}
              onClick={() => {
                if (!deleteTarget) return
                const task = deleteTarget
                void run(task.id, () => api.deleteOrderImportTask(task.id), "任务与图片已删除")
                  .then((deleted) => deleted && setDeleteTarget(null))
              }}
            >
              <Trash2 />删除
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  )
}

function PoolPanel({ pool, provider, disabled, pendingKey, onRetry, onDelete, onRetryAll, onSubmit, onCancelBatch }: {
  pool: OrderImportPool
  provider: AppSettings["orderRecognitionProvider"]
  disabled: boolean
  pendingKey: string | null
  onRetry: (id: string) => void
  onDelete: (id: string) => void
  onRetryAll: () => void
  onSubmit: () => void
  onCancelBatch: (id: string) => void
}) {
  return (
    <section className="flex min-h-0 flex-col bg-white" aria-label="订单识别任务池">
      <div className="flex min-h-20 shrink-0 flex-wrap items-center gap-3 border-b px-5 py-3">
        <div className="min-w-0 flex-1">
          <h2 className="text-sm font-medium">任务池</h2>
          <p className="mt-1 text-xs text-muted-foreground">
            待处理 {pool.pending} · 处理中 {pool.processing} · 失败 {pool.failed}
          </p>
        </div>
        {pool.failed > 0 && (
          <Button variant="outline" size="sm" disabled={disabled} onClick={onRetryAll}>
            <RefreshCw className={cn(pendingKey === "retry-all" && "animate-spin")} />重试全部
          </Button>
        )}
        {provider === "aliyun_batch" && (
          <Button size="sm" disabled={disabled || pool.pending === 0} onClick={onSubmit}>
            <CloudUpload className={cn(pendingKey === "submit" && "animate-pulse")} />提交批次
          </Button>
        )}
      </div>
      {pool.batches.length > 0 && (
        <div className="shrink-0 border-b bg-zinc-50 px-5 py-3">
          <p className="mb-2 text-xs font-medium text-muted-foreground">运行批次</p>
          <div className="flex flex-wrap gap-2">
            {pool.batches.map((batch) => (
              <div key={batch.id} className="flex h-9 items-center gap-2 rounded-md border bg-white px-3 text-xs">
                <Badge variant="outline">{batch.shortNumber}</Badge>
                <LoaderCircle className="size-3.5 animate-spin text-blue-600" />
                <span>{batch.taskCount} 项</span>
                <span className="max-w-32 truncate text-muted-foreground">{batch.model}</span>
                <Button
                  variant="ghost"
                  size="icon-sm"
                  title={`取消批次 ${batch.shortNumber}`}
                  disabled={disabled || batch.status === "canceling"}
                  onClick={() => onCancelBatch(batch.id)}
                >
                  <Ban />
                </Button>
              </div>
            ))}
          </div>
        </div>
      )}
      <div className="grid h-10 shrink-0 grid-cols-[minmax(0,1fr)_120px_96px] items-center gap-3 border-b px-5 text-xs font-medium text-muted-foreground">
        <span>图片</span><span>状态</span><span className="text-right">操作</span>
      </div>
      <ScrollArea className="min-h-0 flex-1">
        {pool.tasks.map((task) => (
          <TaskRow
            key={task.id}
            task={task}
            disabled={disabled}
            pending={pendingKey === task.id}
            onRetry={() => onRetry(task.id)}
            onDelete={() => onDelete(task.id)}
          />
        ))}
        {pool.tasks.length === 0 && (
          <div className="flex h-72 flex-col items-center justify-center gap-3 text-muted-foreground">
            <Clock3 className="size-8" /><p className="text-sm">任务池为空</p>
          </div>
        )}
      </ScrollArea>
    </section>
  )
}

function TaskRow({ task, disabled, pending, onRetry, onDelete }: {
  task: OrderImportTask
  disabled: boolean
  pending: boolean
  onRetry: () => void
  onDelete: () => void
}) {
  const meta = taskStatus[task.status]
  const Icon = meta.icon
  const removable = task.status === "pending" || task.status === "failed"
  return (
    <div className={cn("grid min-h-14 grid-cols-[minmax(0,1fr)_120px_96px] items-center gap-3 border-b px-5 py-2", pending && "opacity-60")}>
      <div className="min-w-0">
        <p className="truncate text-sm font-medium">{task.originalFileName}</p>
        <p className={cn("truncate text-xs", task.errorMessage ? "text-red-600" : "text-muted-foreground")}>
          {task.errorMessage ?? (task.batchShortNumber ? `批次 ${task.batchShortNumber}` : formatDateTime(task.createdAt))}
        </p>
      </div>
      <div className={cn("flex items-center gap-2 text-xs", meta.className)}>
        <Icon className={cn("size-3.5", ["processing", "batch_locked"].includes(task.status) && "animate-spin")} />{meta.label}
      </div>
      <div className="flex justify-end gap-1">
        {task.status === "failed" && (
          <Button variant="ghost" size="icon-sm" title="重试" disabled={disabled} onClick={onRetry}><RotateCcw /></Button>
        )}
        {removable && (
          <Button variant="ghost" size="icon-sm" title="删除任务" disabled={disabled} onClick={onDelete}><Trash2 /></Button>
        )}
      </div>
    </div>
  )
}

function LogsPanel({ logs, page, pageSize, loading, onPageChange, onPageSizeChange }: {
  logs: OrderImportLogsPage
  page: number
  pageSize: number
  loading: boolean
  onPageChange: (value: number) => void
  onPageSizeChange: (value: number) => void
}) {
  const pages = Math.max(1, Math.ceil(logs.total / logs.pageSize))
  return (
    <section className="flex min-h-0 flex-col bg-white" aria-label="订单导入完成日志">
      <div className="flex min-h-20 shrink-0 items-center border-b px-5 py-3">
        <div><h2 className="text-sm font-medium">导入完成</h2><p className="mt-1 text-xs text-muted-foreground">{logs.total} 条不可变识别日志</p></div>
      </div>
      <ScrollArea className={cn("min-h-0 flex-1", loading && "opacity-70")}>
        {logs.items.map((log) => <LogRow key={log.taskId} log={log} />)}
        {!loading && logs.items.length === 0 && (
          <div className="flex h-72 flex-col items-center justify-center gap-3 text-muted-foreground">
            <FileClock className="size-8" /><p className="text-sm">暂无完成记录</p>
          </div>
        )}
      </ScrollArea>
      <div className="flex h-14 shrink-0 items-center justify-between border-t px-4">
        <Select
          value={String(pageSize)}
          onValueChange={(value) => value !== null && onPageSizeChange(Number(value))}
        >
          <SelectTrigger size="sm" className="w-20" aria-label="完成日志每页条数">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {pageSizes.map((size) => <SelectItem key={size} value={String(size)}>{size} 条</SelectItem>)}
          </SelectContent>
        </Select>
        <div className="flex items-center gap-2">
          <span className="text-xs tabular-nums text-muted-foreground">{logs.page} / {pages}</span>
          <Button variant="outline" size="icon-sm" title="上一页" disabled={page <= 1 || loading} onClick={() => onPageChange(page - 1)}><ChevronLeft /></Button>
          <Button variant="outline" size="icon-sm" title="下一页" disabled={page >= pages || loading} onClick={() => onPageChange(page + 1)}><ChevronRight /></Button>
        </div>
      </div>
    </section>
  )
}

function LogRow({ log }: { log: OrderImportLog }) {
  return (
    <div className="grid min-h-16 grid-cols-[minmax(0,1fr)_120px] items-center gap-3 border-b px-5 py-2">
      <div className="min-w-0">
        <div className="flex min-w-0 items-center gap-2">
          <CheckCircle2 className="size-4 shrink-0 text-emerald-600" />
          <p className="truncate text-sm font-medium">{log.originalFileName}</p>
          {log.batchShortNumber && <Badge variant="outline">{log.batchShortNumber}</Badge>}
        </div>
        <p className="mt-1 truncate text-xs text-muted-foreground">{providerLabel(log.provider)} · {log.model}{log.warningMessage ? ` · ${log.warningMessage}` : ""}</p>
      </div>
      <span className="text-right text-xs text-muted-foreground">{formatDateTime(log.completedAt)}</span>
    </div>
  )
}

function providerLabel(provider: string) {
  if (provider === "ollama") return "Ollama"
  if (provider === "openai") return "OpenAI 兼容"
  if (provider === "aliyun_batch") return "阿里云百炼批量"
  return provider
}
