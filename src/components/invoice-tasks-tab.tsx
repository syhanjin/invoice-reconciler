import {
  AlertCircle,
  Ban,
  CheckCircle2,
  ChevronLeft,
  ChevronRight,
  CircleEllipsis,
  Clock3,
  FileCheck2,
  FileClock,
  LoaderCircle,
  RotateCcw,
  ScanSearch,
  XCircle,
} from "lucide-react"
import { useCallback, useEffect, useRef, useState } from "react"
import { toast } from "sonner"

import { Alert, AlertDescription } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Progress } from "@/components/ui/progress"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { api, errorMessage } from "@/lib/api"
import { formatDateTime } from "@/lib/format"
import type { InvoiceBatchDetail, InvoiceBatchesPage, InvoiceBatchSummary, InvoiceTask, InvoiceTaskStatus } from "@/lib/types"
import { cn } from "@/lib/utils"

const emptyPage: InvoiceBatchesPage = { items: [], total: 0, page: 1, pageSize: 20 }
const forcedTypes = [["", "自动"], ["3", "增值税"], ["16", "全电"], ["0", "出租车"], ["1", "定额"], ["2", "火车票"], ["5", "机票"], ["8", "机打"], ["9", "汽车票"], ["10", "轮船票"], ["11", "卷票"], ["12", "购车"], ["13", "路桥费"], ["15", "非税"], ["17", "医疗"], ["18", "完税"], ["19", "海关缴款"], ["20", "银行回单"], ["21", "网约车"], ["22", "报关单"], ["23", "海外"], ["24", "购物小票"], ["25", "销货清单"], ["-1", "其他"]] as const

const statusMeta: Record<InvoiceTaskStatus, { label: string; icon: typeof Clock3; className: string }> = {
  queued: { label: "排队", icon: Clock3, className: "text-zinc-600" },
  processing: { label: "识别中", icon: LoaderCircle, className: "text-blue-600" },
  awaiting_review: { label: "待复核", icon: ScanSearch, className: "text-amber-700" },
  succeeded: { label: "已入库", icon: CheckCircle2, className: "text-emerald-600" },
  failed: { label: "失败", icon: XCircle, className: "text-red-600" },
  canceled: { label: "已取消", icon: Ban, className: "text-zinc-500" },
  interrupted: { label: "已中断", icon: AlertCircle, className: "text-orange-600" },
}

export function InvoiceTasksTab({ activeBatch, selectedBatchId, onSelectedBatchChange, onBatchChange }: { activeBatch: InvoiceBatchDetail | null; selectedBatchId: string | null; onSelectedBatchChange: (id: string | null) => void; onBatchChange: (batch: InvoiceBatchDetail) => void }) {
  const [history, setHistory] = useState<InvoiceBatchesPage>(emptyPage)
  const [page, setPage] = useState(1)
  const [pageSize, setPageSize] = useState(20)
  const [detail, setDetail] = useState<InvoiceBatchDetail | null>(null)
  const [loading, setLoading] = useState(true)
  const [detailLoading, setDetailLoading] = useState(false)
  const [version, setVersion] = useState(0)
  const selectedRef = useRef(selectedBatchId)
  useEffect(() => { selectedRef.current = selectedBatchId }, [selectedBatchId])

  useEffect(() => {
    setLoading(true)
    api.listInvoiceBatches({ page, pageSize }).then((result) => {
      setHistory(result)
      if (!selectedRef.current || !result.items.some((item) => item.id === selectedRef.current)) onSelectedBatchChange(result.items[0]?.id ?? null)
    }).catch((error) => toast.error(errorMessage(error))).finally(() => setLoading(false))
  }, [onSelectedBatchChange, page, pageSize, version])

  useEffect(() => {
    if (!selectedBatchId) { setDetail(null); return }
    setDetailLoading(true)
    api.getInvoiceBatch(selectedBatchId).then(setDetail).catch((error) => toast.error(errorMessage(error))).finally(() => setDetailLoading(false))
  }, [selectedBatchId, version])

  useEffect(() => {
    if (activeBatch?.id !== selectedBatchId) return
    setDetail(activeBatch)
    setHistory((current) => ({ ...current, items: current.items.map((item) => item.id === activeBatch.id ? summary(activeBatch) : item) }))
  }, [activeBatch, selectedBatchId])

  const changeBatch = useCallback((batch: InvoiceBatchDetail) => {
    setDetail(batch)
    setHistory((current) => ({ ...current, items: current.items.map((item) => item.id === batch.id ? summary(batch) : item) }))
    onBatchChange(batch)
  }, [onBatchChange])
  const pages = Math.max(1, Math.ceil(history.total / history.pageSize))

  return <div className="grid h-full min-h-0 grid-rows-[minmax(220px,38%)_minmax(0,1fr)] xl:grid-cols-[minmax(300px,34%)_minmax(0,1fr)] xl:grid-rows-1">
    <section className="flex min-h-0 flex-col border-b bg-white xl:border-r xl:border-b-0">
      <div className="flex h-11 items-center justify-between border-b px-5"><span className="text-xs font-medium text-muted-foreground">发票批次</span><span className="text-xs tabular-nums text-muted-foreground">{history.total} 条</span></div>
      <ScrollArea className={cn("min-h-0 flex-1", loading && "opacity-60")}>{history.items.map((batch) => <HistoryRow key={batch.id} batch={batch} selected={batch.id === selectedBatchId} onClick={() => onSelectedBatchChange(batch.id)} />)}{!loading && !history.items.length && <div className="flex h-72 flex-col items-center justify-center gap-3 text-muted-foreground"><FileClock className="size-8" /><p className="text-sm">暂无发票导入任务</p></div>}</ScrollArea>
      <div className="flex h-14 items-center justify-between border-t px-4"><Select value={String(pageSize)} onValueChange={(value) => { if (!value) return; setPageSize(Number(value)); setPage(1) }}><SelectTrigger size="sm" className="w-20"><SelectValue /></SelectTrigger><SelectContent>{[20, 50, 100].map((size) => <SelectItem key={size} value={String(size)}>{size} 条</SelectItem>)}</SelectContent></Select><div className="flex items-center gap-2"><span className="text-xs">{history.page} / {pages}</span><Button size="icon-sm" variant="outline" disabled={page <= 1 || loading} onClick={() => setPage((value) => value - 1)}><ChevronLeft /></Button><Button size="icon-sm" variant="outline" disabled={page >= pages || loading} onClick={() => setPage((value) => value + 1)}><ChevronRight /></Button></div></div>
    </section>
    <section className="min-h-0 bg-white">{detail ? <InvoiceBatchPanel batch={detail} onBatchChange={changeBatch} onRefresh={() => setVersion((value) => value + 1)} /> : <div className="flex h-full flex-col items-center justify-center gap-3 text-muted-foreground"><CircleEllipsis className={cn("size-8", detailLoading && "animate-pulse")} /><p className="text-sm">{detailLoading ? "正在加载批次" : "请选择一个发票批次"}</p></div>}</section>
  </div>
}

function HistoryRow({ batch, selected, onClick }: { batch: InvoiceBatchSummary; selected: boolean; onClick: () => void }) { const finished = batch.counts.succeeded + batch.counts.failed + batch.counts.canceled + batch.counts.interrupted; const progress = batch.counts.total ? Math.round((finished / batch.counts.total) * 100) : 0; return <button type="button" className={cn("grid w-full gap-3 border-b px-5 py-4 text-left hover:bg-zinc-50", selected && "bg-zinc-100")} onClick={onClick}><div className="flex justify-between gap-3"><span className="text-sm font-medium">{formatDateTime(batch.createdAt)}</span><Badge variant={batch.status === "completed_with_errors" ? "destructive" : "secondary"}>{batch.status === "awaiting_review" ? "待复核" : batch.status === "processing" ? "处理中" : batch.status === "completed" ? "已完成" : batch.status === "completed_with_errors" ? "部分失败" : "排队"}</Badge></div><div className="flex items-center gap-3"><Progress value={progress} className="h-1.5" /><span className="w-9 text-right text-xs">{progress}%</span></div><p className="text-xs text-muted-foreground">共 {batch.counts.total} 个文件 · 成功 {batch.counts.succeeded} · 待复核 {batch.counts.awaitingReview}</p></button> }

function InvoiceBatchPanel({ batch, onBatchChange, onRefresh }: { batch: InvoiceBatchDetail; onBatchChange: (batch: InvoiceBatchDetail) => void; onRefresh: () => void }) {
  const [pending, setPending] = useState<string | null>(null)
  const [retryTypes, setRetryTypes] = useState<Record<string, string>>({})
  const [reviewTask, setReviewTask] = useState<InvoiceTask | null>(null)
  const finished = batch.counts.succeeded + batch.counts.failed + batch.counts.canceled + batch.counts.interrupted
  const progress = batch.counts.total ? Math.round((finished / batch.counts.total) * 100) : 0
  return <div className="flex h-full min-h-0 flex-col"><div className="flex min-h-24 items-center gap-5 border-b px-6 py-4"><div className="min-w-0 flex-1"><div className="mb-2 flex items-center gap-3"><span className="text-sm font-medium">发票导入批次</span>{batch.forcedType !== null && <Badge variant="outline">类型 {batch.forcedType}</Badge>}<span className="text-xs text-muted-foreground">{formatDateTime(batch.createdAt)}</span></div><div className="flex items-center gap-3"><Progress value={progress} className="h-1.5 max-w-md" /><span className="text-xs tabular-nums text-muted-foreground">{finished}/{batch.counts.total}</span></div></div><div className="hidden gap-3 text-xs text-muted-foreground xl:flex"><span>识别中 {batch.counts.processing}</span><span>成功 {batch.counts.succeeded}</span><span>待复核 {batch.counts.awaitingReview}</span><span>失败 {batch.counts.failed}</span></div></div>{batch.counts.awaitingReview > 0 && <Alert className="mx-6 mt-4 rounded-md border-amber-200 bg-amber-50"><ScanSearch /><AlertDescription>有识别项需要确认跨页合并或拆分关系。</AlertDescription></Alert>}<div className="flex h-11 items-center border-b px-6 text-xs font-medium text-muted-foreground">文件任务</div><ScrollArea className="min-h-0 flex-1">{batch.tasks.map((task) => { const meta = statusMeta[task.status]; const Icon = meta.icon; const retryable = (task.status === "failed" || task.status === "interrupted" || task.status === "canceled") && task.sourcePath !== null; const retryValue = retryTypes[task.id] ?? (task.forcedType === null ? "" : String(task.forcedType)); return <div key={task.id} className="grid min-h-14 grid-cols-[minmax(0,1fr)_100px_minmax(130px,220px)] items-center gap-3 border-b px-4 py-2"><div className="min-w-0"><p className="truncate text-sm font-medium">{task.originalFileName}</p>{(task.errorMessage || task.warningMessage) && <p className="truncate text-xs text-muted-foreground">{task.errorMessage || task.warningMessage}</p>}{task.existingInvoiceId && <p className="truncate text-xs text-red-600">已有发票：{task.existingInvoiceId}</p>}</div><div className={cn("flex items-center gap-2 text-xs", meta.className)}><Icon className={cn("size-3.5", task.status === "processing" && "animate-spin")} />{meta.label}</div><div className="flex justify-end gap-2">{retryable && <><Select value={retryValue} onValueChange={(value) => setRetryTypes((current) => ({ ...current, [task.id]: value ?? "" }))}><SelectTrigger size="sm" className="w-24"><SelectValue /></SelectTrigger><SelectContent>{forcedTypes.map(([value, label]) => <SelectItem key={value || "auto"} value={value}>{label}</SelectItem>)}</SelectContent></Select><Button size="icon-sm" variant="ghost" disabled={pending !== null} title="重试" onClick={async () => { setPending(task.id); try { onBatchChange(await api.retryInvoiceTask(task.id, retryValue === "" ? null : Number(retryValue))) } catch (error) { toast.error(errorMessage(error)) } finally { setPending(null) } }}><RotateCcw className={cn(pending === task.id && "animate-spin")} /></Button></>}{task.status === "queued" && <Button size="icon-sm" variant="ghost" title="取消" disabled={pending !== null} onClick={async () => { setPending(task.id); try { await api.cancelInvoiceTask(task.id); onRefresh() } catch (error) { toast.error(errorMessage(error)) } finally { setPending(null) } }}><Ban /></Button>}{task.status === "awaiting_review" && <Button size="sm" variant="outline" onClick={() => setReviewTask(task)}><FileCheck2 />复核</Button>}</div></div> })}</ScrollArea><ReviewDialog task={reviewTask} onOpenChange={(open) => !open && setReviewTask(null)} onConfirmed={(next) => { setReviewTask(null); onBatchChange(next) }} /></div>
}

function ReviewDialog({ task, onOpenChange, onConfirmed }: { task: InvoiceTask | null; onOpenChange: (open: boolean) => void; onConfirmed: (batch: InvoiceBatchDetail) => void }) { const [groups, setGroups] = useState<Record<number, string>>({}); const [saving, setSaving] = useState(false); useEffect(() => { if (task) setGroups(Object.fromEntries(task.reviewItems.map((item, index) => [item.index, String(index + 1)]))) }, [task]); const submit = async () => { if (!task) return; const ordered = new Map<string, number[]>(); for (const item of task.reviewItems) { const group = groups[item.index]?.trim(); if (!group) { toast.error("每个识别项都要填写组号"); return } const values = ordered.get(group) ?? []; values.push(item.index); ordered.set(group, values) } setSaving(true); try { onConfirmed(await api.confirmInvoiceSplit(task.id, [...ordered.values()])) } catch (error) { toast.error(errorMessage(error)) } finally { setSaving(false) } }; return <Dialog open={Boolean(task)} onOpenChange={(open) => !saving && onOpenChange(open)}><DialogContent><DialogHeader><DialogTitle>复核发票分组</DialogTitle><DialogDescription>组号相同的识别项将合并为一张跨页发票；不同组号会拆分为独立发票。确认过程不会再次调用 OCR。</DialogDescription></DialogHeader><div className="max-h-80 overflow-y-auto border">{task?.reviewItems.map((item) => <div key={item.index} className="grid grid-cols-[56px_70px_minmax(0,1fr)] items-center gap-3 border-b px-3 py-2 last:border-0"><Input aria-label={`识别项 ${item.index + 1} 组号`} inputMode="numeric" value={groups[item.index] ?? ""} onChange={(event) => setGroups((current) => ({ ...current, [item.index]: event.target.value }))} /><Badge variant="outline">第 {item.page} 页</Badge><div className="min-w-0"><p className="truncate text-sm">{item.subtypeDescription}</p><p className="truncate text-xs text-muted-foreground">{item.invoiceNumber ?? "未识别号码"}</p></div></div>)}</div><DialogFooter><Button variant="outline" disabled={saving} onClick={() => onOpenChange(false)}>取消</Button><Button disabled={saving} onClick={() => void submit()}>{saving ? <LoaderCircle className="animate-spin" /> : <FileCheck2 />}确认分组</Button></DialogFooter></DialogContent></Dialog> }
function summary(batch: InvoiceBatchDetail): InvoiceBatchSummary {
  return {
    id: batch.id,
    status: batch.status,
    forcedType: batch.forcedType,
    createdAt: batch.createdAt,
    completedAt: batch.completedAt,
    counts: batch.counts,
  }
}
