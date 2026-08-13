import {
  AlertCircle,
  CheckCircle2,
  ChevronLeft,
  ChevronRight,
  Clock3,
  FileClock,
  LoaderCircle,
  Pencil,
  Play,
  RefreshCw,
  Sparkles,
} from "lucide-react"
import { useCallback, useEffect, useState } from "react"
import { toast } from "sonner"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { api, errorMessage } from "@/lib/api"
import { formatDateTime } from "@/lib/format"
import type {
  MatchSummariesPage,
  MatchSummaryAttemptItem,
  MatchSummaryAttemptsPage,
  MatchSummaryListItem,
  MatchSummaryStatus,
} from "@/lib/types"
import { cn } from "@/lib/utils"

const emptySummaries: MatchSummariesPage = {
  items: [],
  counts: { total: 0, notGenerated: 0, pending: 0, processing: 0, succeeded: 0, failed: 0 },
  total: 0,
  page: 1,
  pageSize: 20,
}
const emptyAttempts: MatchSummaryAttemptsPage = { items: [], total: 0, page: 1, pageSize: 20 }
const statuses: Array<{ value: MatchSummaryStatus | "all"; label: string }> = [
  { value: "all", label: "全部状态" },
  { value: "not_generated", label: "未生成" },
  { value: "pending", label: "待生成" },
  { value: "processing", label: "生成中" },
  { value: "succeeded", label: "已完成" },
  { value: "failed", label: "失败" },
]

export function SummaryTasksTab() {
  const [summaries, setSummaries] = useState(emptySummaries)
  const [attempts, setAttempts] = useState(emptyAttempts)
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [searchDraft, setSearchDraft] = useState("")
  const [search, setSearch] = useState("")
  const [status, setStatus] = useState<MatchSummaryStatus | "all">("all")
  const [page, setPage] = useState(1)
  const [attemptPage, setAttemptPage] = useState(1)
  const [loading, setLoading] = useState(true)
  const [pending, setPending] = useState<string | null>(null)
  const [editTarget, setEditTarget] = useState<MatchSummaryListItem | null>(null)
  const [editValue, setEditValue] = useState("")
  const [version, setVersion] = useState(0)

  const loadSummaries = useCallback(async () => {
    try {
      const result = await api.listMatchSummaries({ page, pageSize: 20, search, status })
      const pages = Math.max(1, Math.ceil(result.total / result.pageSize))
      if (page > pages) setPage(pages)
      else {
        setSummaries(result)
        setSelectedId((current) => result.items.some((item) => item.invoiceId === current) ? current : result.items[0]?.invoiceId ?? null)
      }
    } catch (error) {
      toast.error(errorMessage(error))
    }
  }, [page, search, status])

  const loadAttempts = useCallback(async () => {
    if (!selectedId) {
      setAttempts(emptyAttempts)
      return
    }
    try {
      const result = await api.listMatchSummaryAttempts({ invoiceId: selectedId, page: attemptPage, pageSize: 20 })
      const pages = Math.max(1, Math.ceil(result.total / result.pageSize))
      if (attemptPage > pages) setAttemptPage(pages)
      else setAttempts(result)
    } catch (error) {
      toast.error(errorMessage(error))
    }
  }, [attemptPage, selectedId])

  useEffect(() => {
    setLoading(true)
    loadSummaries().finally(() => setLoading(false))
  }, [loadSummaries, version])

  useEffect(() => {
    void loadAttempts()
  }, [loadAttempts, version])

  useEffect(() => {
    let dispose: undefined | (() => void)
    api.listenForMatchSummaryUpdates(() => setVersion((value) => value + 1)).then((unlisten) => { dispose = unlisten })
    return () => dispose?.()
  }, [])

  const run = async (key: string, action: () => Promise<unknown>, success: string) => {
    setPending(key)
    try {
      await action()
      toast.success(success)
      setVersion((value) => value + 1)
      return true
    } catch (error) {
      toast.error(errorMessage(error), { duration: 6000 })
      return false
    } finally {
      setPending(null)
    }
  }

  const currentPanel = (
    <section className="flex min-h-0 flex-col bg-white" aria-label="摘要当前任务">
      <div className="flex min-h-24 shrink-0 flex-wrap items-center gap-3 border-b px-5 py-3">
        <div className="min-w-0 flex-1">
          <h2 className="text-sm font-medium">当前摘要</h2>
          <p className="mt-1 text-xs text-muted-foreground">未生成 {summaries.counts.notGenerated} · 待生成 {summaries.counts.pending} · 生成中 {summaries.counts.processing} · 失败 {summaries.counts.failed} · 已完成 {summaries.counts.succeeded}</p>
        </div>
        <Button variant="outline" size="sm" disabled={pending !== null || summaries.counts.notGenerated + summaries.counts.failed === 0} onClick={() => void run("all", api.enqueueAllMatchSummaries, "摘要已批量排队")}>
          <Play className={cn(pending === "all" && "animate-pulse")} />生成/重试全部
        </Button>
        <form className="flex w-full gap-2" onSubmit={(event) => { event.preventDefault(); setPage(1); setSearch(searchDraft.trim()) }}>
          <Input value={searchDraft} onChange={(event) => setSearchDraft(event.target.value)} placeholder="搜索发票、销售方、项目或摘要" className="min-w-48 flex-1" />
          <Select value={status} onValueChange={(value) => { setStatus(value as MatchSummaryStatus | "all"); setPage(1) }}>
            <SelectTrigger className="w-32"><SelectValue>{statuses.find((item) => item.value === status)?.label}</SelectValue></SelectTrigger>
            <SelectContent>{statuses.map((item) => <SelectItem key={item.value} value={item.value}>{item.label}</SelectItem>)}</SelectContent>
          </Select>
          <Button type="submit" variant="outline">搜索</Button>
        </form>
      </div>
      <div className="grid h-10 shrink-0 grid-cols-[minmax(0,1fr)_110px_96px] items-center gap-3 border-b px-5 text-xs font-medium text-muted-foreground"><span>组合</span><span>状态</span><span className="text-right">操作</span></div>
      <ScrollArea className={cn("min-h-0 flex-1", loading && "opacity-70")}>
        {summaries.items.map((item) => (
          <div key={item.invoiceId} className={cn("grid min-h-20 w-full grid-cols-[minmax(0,1fr)_110px_96px] items-center gap-3 border-b px-5 py-2 text-left hover:bg-zinc-50", selectedId === item.invoiceId && "bg-zinc-100")}>
            <button type="button" className="col-span-2 grid h-full min-w-0 grid-cols-[minmax(0,1fr)_110px] items-center gap-3 text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring" onClick={() => { setSelectedId(item.invoiceId); setAttemptPage(1) }}>
              <span className="min-w-0"><span className="block truncate text-sm font-medium">{item.invoiceNumber ?? "未识别号码"} · {item.displayName}</span><span className="mt-1 block truncate text-xs text-muted-foreground">{item.projectName ?? "未分配项目"} · {item.sellerName ?? "销售方缺失"}{item.summaryModel ? ` · ${item.summaryModel}` : ""}</span></span>
              <SummaryBadge status={item.summaryStatus} source={item.summarySource} />
            </button>
            <span className="flex justify-end gap-1">
              {item.summaryStatus !== "processing" && item.summaryStatus !== "pending" && <Button variant="ghost" size="icon-sm" title={item.summaryStatus === "failed" ? "重试" : "生成摘要"} disabled={pending !== null} onClick={() => void run(item.invoiceId, () => api.enqueueMatchSummary(item.invoiceId), "摘要已排队")}>{item.summaryStatus === "failed" ? <RefreshCw /> : <Play />}</Button>}
              {item.summaryStatus === "succeeded" && <Button variant="ghost" size="icon-sm" title="编辑摘要" disabled={pending !== null} onClick={() => { setEditTarget(item); setEditValue(item.displayName) }}><Pencil /></Button>}
            </span>
          </div>
        ))}
        {!loading && summaries.items.length === 0 && <Empty icon={Sparkles} label="暂无摘要任务" />}
      </ScrollArea>
      <Pagination page={summaries.page} total={summaries.total} pageSize={summaries.pageSize} disabled={loading} onChange={setPage} />
    </section>
  )

  const attemptsPanel = (
    <section className="flex min-h-0 flex-col bg-white" aria-label="摘要生成记录">
      <div className="flex min-h-20 shrink-0 items-center border-b px-5 py-3"><div><h2 className="text-sm font-medium">生成记录</h2><p className="mt-1 text-xs text-muted-foreground">{selectedId ? `${attempts.total} 次尝试，保留最近 180 天` : "请先选择一个摘要组合"}</p></div></div>
      <ScrollArea className={cn("min-h-0 flex-1", loading && "opacity-70")}>
        {attempts.items.map((attempt) => <AttemptRow key={attempt.id} attempt={attempt} />)}
        {!loading && attempts.items.length === 0 && <Empty icon={FileClock} label={selectedId ? "暂无生成记录" : "尚未选择摘要"} />}
      </ScrollArea>
      <Pagination page={attempts.page} total={attempts.total} pageSize={attempts.pageSize} disabled={loading || !selectedId} onChange={setAttemptPage} />
    </section>
  )

  return (
    <>
      <div className="hidden h-full min-h-0 grid-cols-[minmax(0,1.05fr)_minmax(380px,.95fr)] divide-x xl:grid">{currentPanel}{attemptsPanel}</div>
      <Tabs defaultValue="current" className="h-full min-h-0 gap-0 xl:hidden">
        <div className="flex h-11 items-end border-b bg-white px-4"><TabsList variant="line"><TabsTrigger value="current">当前摘要 {summaries.total}</TabsTrigger><TabsTrigger value="attempts">生成记录 {attempts.total}</TabsTrigger></TabsList></div>
        <TabsContent value="current" className="min-h-0">{currentPanel}</TabsContent><TabsContent value="attempts" className="min-h-0">{attemptsPanel}</TabsContent>
      </Tabs>
      <Dialog open={editTarget !== null} onOpenChange={(open) => !open && pending === null && setEditTarget(null)}>
        <DialogContent>
          <DialogHeader><DialogTitle>编辑商品摘要</DialogTitle><DialogDescription>{editTarget?.invoiceNumber ?? editTarget?.invoiceId}，单行且最多 60 个字符。人工摘要不会被自动任务覆盖。</DialogDescription></DialogHeader>
          <Input aria-label="商品摘要" maxLength={60} value={editValue} onChange={(event) => setEditValue(event.target.value)} />
          <div className="text-right text-xs tabular-nums text-muted-foreground">{Array.from(editValue).length}/60</div>
          <DialogFooter><Button variant="outline" disabled={pending !== null} onClick={() => setEditTarget(null)}>取消</Button><Button disabled={pending !== null || !editValue.trim() || /[\r\n]/.test(editValue)} onClick={() => { if (!editTarget) return; void run(`edit-${editTarget.invoiceId}`, () => api.updateMatchSummary(editTarget.invoiceId, editValue), "摘要已更新").then((saved) => { if (saved) setEditTarget(null) }) }}>保存</Button></DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  )
}

function SummaryBadge({ status, source }: { status: MatchSummaryStatus; source: "generated" | "manual" | null }) {
  if (status === "not_generated") return <Badge variant="outline"><Clock3 />未生成</Badge>
  if (status === "pending") return <Badge variant="secondary"><Clock3 />待生成</Badge>
  if (status === "processing") return <Badge variant="secondary"><LoaderCircle className="animate-spin" />生成中</Badge>
  if (status === "failed") return <Badge variant="destructive"><AlertCircle />失败</Badge>
  return <Badge variant="outline"><CheckCircle2 />{source === "manual" ? "人工" : "已完成"}</Badge>
}

function AttemptRow({ attempt }: { attempt: MatchSummaryAttemptItem }) {
  const [open, setOpen] = useState(false)
  return (
    <div className="border-b px-5 py-3">
      <button type="button" className="flex w-full items-start gap-3 text-left" onClick={() => setOpen((value) => !value)}>
        <span className="min-w-0 flex-1"><span className="flex items-center gap-2"><AttemptBadge status={attempt.status} /><span className="truncate text-sm font-medium">{attempt.resultSummary ?? attempt.errorMessage ?? "正在生成"}</span></span><span className="mt-1 block truncate text-xs text-muted-foreground">{triggerLabel(attempt.trigger)} · {attempt.provider ? `${attempt.provider} / ${attempt.model}` : "人工编辑"} · {formatDateTime(attempt.startedAt)}</span></span>
        <span className="text-xs text-muted-foreground">{open ? "收起" : "详情"}</span>
      </button>
      {open && <div className="mt-3 space-y-3 border-t pt-3 text-xs"><div><p className="font-medium">发票输入</p><p className="mt-1 text-muted-foreground">销售方：{attempt.input.sellerName ?? "-"}</p>{attempt.input.invoiceItems.map((item, index) => <p key={`${attempt.id}-invoice-${index}`} className="mt-1 text-muted-foreground">{index + 1}. {item.name}{item.specification ? ` · ${item.specification}` : ""}{item.quantity ? ` · ${item.quantity}` : ""}</p>)}</div><div><p className="font-medium">订单输入</p>{attempt.input.orders.map((order, index) => <p key={`${attempt.id}-order-${index}`} className="mt-1 text-muted-foreground">{index + 1}. {order.shopName ?? "店铺缺失"} · {order.productNames.join("、") || "商品缺失"}</p>)}</div></div>}
    </div>
  )
}

function AttemptBadge({ status }: { status: MatchSummaryAttemptItem["status"] }) {
  const labels = { processing: "生成中", succeeded: "成功", failed: "失败", interrupted: "已中断", discarded: "已丢弃", manual_edit: "人工编辑" }
  return <Badge variant={status === "failed" || status === "interrupted" ? "destructive" : "outline"}>{labels[status]}</Badge>
}

function Pagination({ page, total, pageSize, disabled, onChange }: { page: number; total: number; pageSize: number; disabled: boolean; onChange: (page: number) => void }) {
  const pages = Math.max(1, Math.ceil(total / pageSize))
  return <div className="flex h-14 shrink-0 items-center justify-end gap-2 border-t px-4"><span className="mr-auto text-xs text-muted-foreground">共 {total} 条</span><span className="text-xs tabular-nums text-muted-foreground">{page} / {pages}</span><Button variant="outline" size="icon-sm" title="上一页" disabled={disabled || page <= 1} onClick={() => onChange(page - 1)}><ChevronLeft /></Button><Button variant="outline" size="icon-sm" title="下一页" disabled={disabled || page >= pages} onClick={() => onChange(page + 1)}><ChevronRight /></Button></div>
}

function Empty({ icon: Icon, label }: { icon: typeof FileClock; label: string }) {
  return <div className="flex h-72 flex-col items-center justify-center gap-3 text-muted-foreground"><Icon className="size-8" /><p className="text-sm">{label}</p></div>
}

function triggerLabel(trigger: string) {
  const labels: Record<string, string> = { automatic: "自动生成", automatic_confirmation: "匹配确认", automatic_input_change: "输入变化", interrupted: "中断恢复", manual_request: "单条生成", manual_batch: "批量生成", retry: "失败重试", manual_edit: "人工编辑" }
  return labels[trigger] ?? trigger
}
