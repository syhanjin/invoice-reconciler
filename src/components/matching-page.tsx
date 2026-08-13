import {
  AlertTriangle,
  Check,
  ChevronLeft,
  ChevronRight,
  GitCompareArrows,
  LoaderCircle,
  RefreshCw,
  Search,
  Settings2,
  Sparkles,
  Unlink,
} from "lucide-react"
import { useCallback, useEffect, useMemo, useState } from "react"
import { toast } from "sonner"

import InvoiceSheet from "@/components/invoice-sheet"
import OrderSheet from "@/components/order-sheet"
import { Badge } from "@/components/ui/badge"
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
import { Skeleton } from "@/components/ui/skeleton"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import { api, errorMessage } from "@/lib/api"
import { formatDateTime } from "@/lib/format"
import type {
  ConfirmInvoiceMatchRequest,
  MatchCandidate,
  MatchableOrderItem,
  MatchableOrdersPage,
  MatchingInvoiceItem,
  MatchingPageData,
  MatchingStatus,
} from "@/lib/types"

const emptyPage: MatchingPageData = {
  items: [],
  summary: { total: 0, suggested: 0, conflict: 0, unmatched: 0, ineligible: 0, confirmed: 0 },
  total: 0,
  page: 1,
  pageSize: 50,
}

const statusOptions: { value: MatchingStatus | "all"; label: string }[] = [
  { value: "all", label: "全部状态" },
  { value: "suggested", label: "待确认" },
  { value: "conflict", label: "冲突" },
  { value: "unmatched", label: "无法自动匹配" },
  { value: "ineligible", label: "不参与匹配" },
  { value: "confirmed", label: "已确认" },
]

export function MatchingPage({ refreshToken }: { refreshToken: string }) {
  const [data, setData] = useState<MatchingPageData>(emptyPage)
  const [page, setPage] = useState(1)
  const [status, setStatus] = useState<MatchingStatus | "all">("all")
  const [draftSearch, setDraftSearch] = useState("")
  const [search, setSearch] = useState("")
  const [loading, setLoading] = useState(true)
  const [submitting, setSubmitting] = useState(false)
  const [selectedSuggestions, setSelectedSuggestions] = useState<string[]>([])
  const [customInvoice, setCustomInvoice] = useState<MatchingInvoiceItem | null>(null)
  const [selectedInvoiceId, setSelectedInvoiceId] = useState<string | null>(null)
  const [selectedOrderId, setSelectedOrderId] = useState<string | null>(null)

  const load = useCallback(async () => {
    setLoading(true)
    try {
      const next = await api.getMatchingPage({ page, pageSize: 50, search, status })
      if (next.items.length === 0 && next.total > 0 && page > 1) {
        setPage(Math.max(1, Math.ceil(next.total / 50)))
        return
      }
      setData(next)
      setSelectedSuggestions((current) => current.filter((id) => next.items.some((item) => item.id === id && item.status === "suggested")))
    } catch (error) {
      toast.error(errorMessage(error))
    } finally {
      setLoading(false)
    }
  }, [page, search, status])

  useEffect(() => { void load() }, [load, refreshToken])

  const suggestedItems = data.items.filter((item) => item.status === "suggested")
  const allSuggestedSelected = suggestedItems.length > 0 && suggestedItems.every((item) => selectedSuggestions.includes(item.id))
  const someSuggestedSelected = suggestedItems.some((item) => selectedSuggestions.includes(item.id))

  const confirmOne = async (item: MatchingInvoiceItem, candidate: MatchCandidate) => {
    setSubmitting(true)
    try {
      await api.confirmInvoiceMatch({ invoiceId: item.id, orderIds: candidate.orderIds, source: "automatic", acceptMismatches: false })
      toast.success("匹配已确认")
      await load()
    } catch (error) {
      toast.error(errorMessage(error))
    } finally {
      setSubmitting(false)
    }
  }

  const confirmSelected = async () => {
    const matches = selectedSuggestions.flatMap((id): ConfirmInvoiceMatchRequest[] => {
      const item = data.items.find((value) => value.id === id)
      const candidate = item?.status === "suggested" ? item.candidates[0] : undefined
      return item && candidate ? [{ invoiceId: item.id, orderIds: candidate.orderIds, source: "automatic", acceptMismatches: false }] : []
    })
    if (!matches.length) return
    setSubmitting(true)
    try {
      await api.confirmInvoiceMatches(matches)
      setSelectedSuggestions([])
      toast.success(`已确认 ${matches.length} 个匹配`)
      await load()
    } catch (error) {
      toast.error(errorMessage(error))
      await load()
    } finally {
      setSubmitting(false)
    }
  }

  const unconfirm = async (id: string) => {
    setSubmitting(true)
    try {
      await api.unconfirmInvoiceMatch(id)
      toast.success("已撤销匹配确认")
      await load()
    } catch (error) {
      toast.error(errorMessage(error))
    } finally {
      setSubmitting(false)
    }
  }

  const retrySummary = async (id: string) => {
    setSubmitting(true)
    try {
      await api.retryMatchSummary(id)
      toast.success("商品摘要已重新排队")
      await load()
    } catch (error) {
      toast.error(errorMessage(error))
    } finally {
      setSubmitting(false)
    }
  }

  const totalPages = Math.max(1, Math.ceil(data.total / data.pageSize))

  return (
    <div className="flex h-screen min-w-0 flex-col bg-white">
      <header className="flex shrink-0 items-start justify-between border-b px-6 py-5">
        <div>
          <h1 className="flex items-center gap-2 text-xl font-semibold"><GitCompareArrows className="size-5" />订单与发票匹配</h1>
          <p className="mt-1 text-sm text-muted-foreground">确认前建议会随订单和发票数据重新计算。</p>
        </div>
        <Button variant="outline" size="sm" disabled={loading} onClick={() => void load()}><RefreshCw className={loading ? "animate-spin" : ""} />刷新</Button>
      </header>

      <section className="grid shrink-0 grid-cols-6 border-b bg-zinc-50">
        <Summary label="全部发票" value={data.summary.total} />
        <Summary label="待确认" value={data.summary.suggested} tone="text-emerald-700" />
        <Summary label="冲突" value={data.summary.conflict} tone="text-amber-700" />
        <Summary label="无法自动匹配" value={data.summary.unmatched} />
        <Summary label="不参与" value={data.summary.ineligible} />
        <Summary label="已确认" value={data.summary.confirmed} tone="text-blue-700" />
      </section>

      <div className="flex h-12 shrink-0 items-center gap-3 border-b px-6">
        <form className="relative w-72" onSubmit={(event) => { event.preventDefault(); setPage(1); setSearch(draftSearch.trim()) }}>
          <Search className="absolute left-2.5 top-2.5 size-4 text-muted-foreground" />
          <Input className="h-9 pl-8" value={draftSearch} onChange={(event) => setDraftSearch(event.target.value)} placeholder="搜索发票号码、销售方或票种" />
        </form>
        <Select value={status} onValueChange={(value) => { setPage(1); setStatus(value as MatchingStatus | "all"); setSelectedSuggestions([]) }}>
          <SelectTrigger className="w-40"><SelectValue>{statusOptions.find((option) => option.value === status)?.label}</SelectValue></SelectTrigger>
          <SelectContent>{statusOptions.map((option) => <SelectItem key={option.value} value={option.value}>{option.label}</SelectItem>)}</SelectContent>
        </Select>
        {selectedSuggestions.length > 0 && <Button size="sm" disabled={submitting} onClick={() => void confirmSelected()}>{submitting ? <LoaderCircle className="animate-spin" /> : <Check />}确认选中的 {selectedSuggestions.length} 项</Button>}
        <span className="ml-auto text-xs text-muted-foreground">第 {data.page} / {totalPages} 页</span>
        <Button variant="outline" size="icon-sm" disabled={page <= 1 || loading} aria-label="上一页" onClick={() => setPage((value) => value - 1)}><ChevronLeft /></Button>
        <Button variant="outline" size="icon-sm" disabled={page >= totalPages || loading} aria-label="下一页" onClick={() => setPage((value) => value + 1)}><ChevronRight /></Button>
      </div>

      <div className="min-h-0 flex-1 overflow-auto">
        <div className="sticky top-0 z-20 grid min-w-[980px] grid-cols-2 border-b bg-zinc-100 text-xs font-medium">
          <div className="flex h-10 items-center gap-3 border-r px-4"><Checkbox checked={allSuggestedSelected} indeterminate={!allSuggestedSelected && someSuggestedSelected} aria-label="选择本页无冲突建议" onCheckedChange={(checked) => setSelectedSuggestions(checked === true ? suggestedItems.map((item) => item.id) : [])} />发票</div>
          <div className="flex h-10 items-center justify-between px-4"><span>订单组合</span><span className="text-muted-foreground">金额按分精确比较</span></div>
        </div>
        <div className={loading ? "opacity-50" : ""}>
          {loading && !data.items.length ? Array.from({ length: 5 }, (_, index) => <Skeleton key={index} className="m-4 h-28" />) : data.items.map((item) => (
            <MatchingRow
              key={item.id}
              item={item}
              selected={selectedSuggestions.includes(item.id)}
              disabled={submitting}
              onSelected={(checked) => setSelectedSuggestions((current) => checked ? [...current, item.id] : current.filter((id) => id !== item.id))}
              onOpenInvoice={() => setSelectedInvoiceId(item.id)}
              onOpenOrder={setSelectedOrderId}
              onConfirm={(candidate) => void confirmOne(item, candidate)}
              onCustom={() => setCustomInvoice(item)}
              onUnconfirm={() => void unconfirm(item.id)}
              onRetrySummary={() => void retrySummary(item.id)}
            />
          ))}
          {!loading && !data.items.length && <div className="flex h-64 flex-col items-center justify-center gap-3 text-muted-foreground"><GitCompareArrows className="size-8" /><p className="text-sm">当前筛选下没有发票</p></div>}
        </div>
      </div>

      <InvoiceSheet invoiceId={selectedInvoiceId} onOpenChange={(open) => !open && setSelectedInvoiceId(null)} onChanged={() => void load()} />
      <OrderSheet orderId={selectedOrderId} onOpenChange={(open) => !open && setSelectedOrderId(null)} onChanged={() => void load()} />
      <CustomMatchDialog invoice={customInvoice} onOpenChange={(open) => !open && setCustomInvoice(null)} onOpenOrder={setSelectedOrderId} onConfirmed={async () => { setCustomInvoice(null); await load() }} />
    </div>
  )
}

function MatchingRow({ item, selected, disabled, onSelected, onOpenInvoice, onOpenOrder, onConfirm, onCustom, onUnconfirm, onRetrySummary }: {
  item: MatchingInvoiceItem
  selected: boolean
  disabled: boolean
  onSelected: (value: boolean) => void
  onOpenInvoice: () => void
  onOpenOrder: (id: string) => void
  onConfirm: (candidate: MatchCandidate) => void
  onCustom: () => void
  onUnconfirm: () => void
  onRetrySummary: () => void
}) {
  const [candidateIndex, setCandidateIndex] = useState(0)
  useEffect(() => setCandidateIndex(0), [item.id, item.candidates.length])
  const candidate = item.candidates[candidateIndex]
  return (
    <div className="grid min-w-[980px] grid-cols-2 border-b">
      <div className="flex min-h-32 gap-3 border-r p-4">
        <div className="pt-1"><Checkbox checked={selected} disabled={item.status !== "suggested"} aria-label={`选择发票 ${item.invoiceNumber ?? item.id}`} onCheckedChange={(value) => onSelected(value === true)} /></div>
        <button type="button" className="min-w-0 flex-1 text-left" onClick={onOpenInvoice}>
          <div className="flex flex-wrap items-center gap-2"><span className="font-medium">{item.invoiceNumber ?? "未识别号码"}</span><StatusBadge status={item.status} /><Badge variant="outline">{item.subtypeDescription}</Badge></div>
          <div className="mt-3 grid grid-cols-2 gap-x-4 gap-y-1 text-xs"><Meta label="销售方" value={item.sellerName ?? "-"} /><Meta label="开票日期" value={item.invoiceDate ?? "-"} /><Meta label="价税合计" value={money(item.totalFen, item.currency)} /><Meta label="确认时间" value={item.confirmedAt ? formatDateTime(item.confirmedAt) : "-"} /></div>
          {item.reason && <p className="mt-3 flex items-start gap-1.5 text-xs text-amber-700"><AlertTriangle className="mt-0.5 size-3 shrink-0" />{item.reason}</p>}
        </button>
      </div>
      <div className="min-h-32 p-4">
        {candidate ? <>
          {item.status === "confirmed" && <div className="mb-3 flex min-h-7 items-center gap-2"><Sparkles className="size-4 text-emerald-700" /><span className="min-w-0 flex-1 truncate text-sm font-medium">{item.displayName ?? "订单组合"}</span>{item.summaryStatus === "pending" && <Badge variant="secondary">待生成</Badge>}{item.summaryStatus === "processing" && <Badge variant="secondary"><LoaderCircle className="animate-spin" />生成中</Badge>}{item.summaryStatus === "failed" && <Tooltip><TooltipTrigger render={<Button type="button" size="icon-xs" variant="outline" aria-label={`重试商品摘要 ${item.invoiceNumber ?? item.id}`} disabled={disabled} onClick={onRetrySummary} />}><RefreshCw /></TooltipTrigger><TooltipContent>{item.summaryError ?? "商品摘要生成失败，点击重试"}</TooltipContent></Tooltip>}</div>}
          {item.candidates.length > 1 && <div className="mb-3 flex flex-wrap gap-2">{item.candidates.map((value, index) => <Button key={value.orderIds.join(":")} size="xs" variant={index === candidateIndex ? "default" : "outline"} onClick={() => setCandidateIndex(index)}>组合 {index + 1}</Button>)}</div>}
          <div className="divide-y border">{candidate.orders.map((order) => <button type="button" key={order.id} className="grid w-full grid-cols-[minmax(150px,1fr)_130px_110px] gap-3 px-3 py-2 text-left text-xs hover:bg-zinc-50" onClick={() => onOpenOrder(order.id)}><span className="truncate"><strong className="font-medium">{order.externalOrderNumber ?? order.id}</strong><span className="ml-2 text-muted-foreground">{order.productSummary}</span></span><span className="truncate">{order.shopName ?? "-"}</span><span className="text-right tabular-nums">{money(order.paidAmountFen)}</span></button>)}</div>
          <div className="mt-3 flex items-center justify-between"><span className="text-xs text-muted-foreground">{candidate.orders.length} 张订单，合计 {money(candidate.totalFen)}</span><div className="flex gap-2">{item.status !== "confirmed" && item.status !== "ineligible" && <Button size="sm" variant="outline" onClick={onCustom}><Settings2 />自定义</Button>}{item.status !== "confirmed" && <Button size="sm" disabled={disabled} onClick={() => onConfirm(candidate)}><Check />确认组合</Button>}{item.status === "confirmed" && <Button size="sm" variant="outline" disabled={disabled} onClick={onUnconfirm}><Unlink />撤销确认</Button>}</div></div>
        </> : <div className="flex h-full min-h-24 items-center justify-between gap-4"><span className="text-sm text-muted-foreground">{item.status === "ineligible" ? "请先补齐或修正发票信息" : "没有可直接确认的自动候选组合"}</span>{(item.status === "unmatched" || item.status === "conflict") && <Button size="sm" variant="outline" onClick={onCustom}><Settings2 />自定义组合</Button>}</div>}
      </div>
    </div>
  )
}

function CustomMatchDialog({ invoice, onOpenChange, onOpenOrder, onConfirmed }: { invoice: MatchingInvoiceItem | null; onOpenChange: (open: boolean) => void; onOpenOrder: (id: string) => void; onConfirmed: () => Promise<void> }) {
  const [data, setData] = useState<MatchableOrdersPage>({ items: [], total: 0, page: 1, pageSize: 20 })
  const [page, setPage] = useState(1)
  const [draftSearch, setDraftSearch] = useState("")
  const [search, setSearch] = useState("")
  const [selected, setSelected] = useState<Record<string, MatchableOrderItem>>({})
  const [loading, setLoading] = useState(false)
  const [submitting, setSubmitting] = useState(false)
  const [differences, setDifferences] = useState<string[]>([])

  useEffect(() => {
    setPage(1)
    setDraftSearch("")
    setSearch("")
    setSelected({})
    setDifferences([])
  }, [invoice?.id])

  useEffect(() => {
    if (!invoice) return
    setLoading(true)
    api.listMatchableOrders({ page, pageSize: 20, search, sortBy: "created_at", sortDirection: "desc" })
      .then(setData).catch((error) => toast.error(errorMessage(error))).finally(() => setLoading(false))
  }, [invoice, page, search])

  const selectedItems = useMemo(() => Object.values(selected), [selected])
  const selectedTotal = selectedItems.reduce((total, item) => total + (item.paidAmountFen ?? 0), 0)
  const submit = async (acceptMismatches: boolean) => {
    if (!invoice || !selectedItems.length) return
    setSubmitting(true)
    try {
      const result = await api.confirmInvoiceMatch({ invoiceId: invoice.id, orderIds: selectedItems.map((item) => item.id), source: "custom", acceptMismatches })
      if (!result.confirmed) { setDifferences(result.differences); return }
      toast.success("自定义匹配已确认")
      await onConfirmed()
    } catch (error) {
      toast.error(errorMessage(error))
    } finally {
      setSubmitting(false)
    }
  }

  const totalPages = Math.max(1, Math.ceil(data.total / data.pageSize))
  return <Dialog open={Boolean(invoice)} onOpenChange={onOpenChange}><DialogContent className="flex h-[min(820px,calc(100vh-32px))] flex-col gap-0 overflow-hidden p-0" style={{ width: "min(1100px, calc(100vw - 32px))", maxWidth: "none" }} showCloseButton>
    <DialogHeader className="shrink-0 border-b p-5"><DialogTitle>自定义订单组合</DialogTitle><DialogDescription>发票 {invoice?.invoiceNumber ?? invoice?.id}，目标金额 {money(invoice?.totalFen ?? null, invoice?.currency)}</DialogDescription></DialogHeader>
    <div className="grid min-h-0 flex-1 grid-cols-[minmax(0,1fr)_300px]">
      <div className="flex min-h-0 flex-col border-r">
        <form className="flex h-12 shrink-0 items-center gap-2 border-b px-4" onSubmit={(event) => { event.preventDefault(); setPage(1); setSearch(draftSearch.trim()) }}><Search className="size-4 text-muted-foreground" /><Input className="h-8" value={draftSearch} onChange={(event) => setDraftSearch(event.target.value)} placeholder="搜索订单号、店铺或商品" /><Button size="sm" variant="outline">搜索</Button></form>
        <div className="min-h-0 flex-1 overflow-auto"><Table><TableHeader className="sticky top-0 z-10 bg-zinc-50"><TableRow><TableHead className="w-10" /><TableHead>订单</TableHead><TableHead>店铺</TableHead><TableHead>下单日期</TableHead><TableHead className="text-right">实付款</TableHead></TableRow></TableHeader><TableBody className={loading ? "opacity-50" : undefined}>{data.items.map((item) => { const disabled = Boolean(item.disabledReason); return <TableRow key={item.id} data-state={selected[item.id] ? "selected" : undefined} className={disabled ? "opacity-50" : "cursor-pointer"} onClick={() => !disabled && onOpenOrder(item.id)}><TableCell onClick={(event) => event.stopPropagation()}><Checkbox checked={Boolean(selected[item.id])} disabled={disabled} aria-label={`选择订单 ${item.externalOrderNumber ?? item.id}`} onCheckedChange={(checked) => { setDifferences([]); setSelected((current) => { const next = { ...current }; if (checked === true) next[item.id] = item; else delete next[item.id]; return next }) }} /></TableCell><TableCell><p className="font-medium">{item.externalOrderNumber ?? item.id}</p><p className="max-w-56 truncate text-xs text-muted-foreground">{item.disabledReason ?? item.productSummary}</p></TableCell><TableCell className="max-w-36 truncate">{item.shopName ?? "-"}</TableCell><TableCell>{item.orderedAt ?? "-"}</TableCell><TableCell className="text-right tabular-nums">{money(item.paidAmountFen)}</TableCell></TableRow>})}</TableBody></Table></div>
        <div className="flex h-11 shrink-0 items-center justify-end gap-2 border-t px-4 text-xs"><span className="mr-auto text-muted-foreground">第 {page} / {totalPages} 页</span><Button variant="outline" size="icon-xs" disabled={page <= 1} onClick={() => setPage((value) => value - 1)}><ChevronLeft /></Button><Button variant="outline" size="icon-xs" disabled={page >= totalPages} onClick={() => setPage((value) => value + 1)}><ChevronRight /></Button></div>
      </div>
      <aside className="min-h-0 overflow-y-auto p-4"><Label>已选订单</Label><div className="mt-3 divide-y border">{selectedItems.map((item) => <div key={item.id} className="flex items-center justify-between gap-2 px-3 py-2 text-xs"><span className="truncate">{item.externalOrderNumber ?? item.id}</span><span className="tabular-nums">{money(item.paidAmountFen)}</span></div>)}{!selectedItems.length && <p className="px-3 py-8 text-center text-xs text-muted-foreground">可跨页选择订单</p>}</div><div className="mt-4 space-y-2 border-t pt-4 text-sm"><div className="flex justify-between"><span>订单数</span><strong>{selectedItems.length}</strong></div><div className="flex justify-between"><span>订单合计</span><strong>{money(selectedTotal)}</strong></div><div className="flex justify-between"><span>金额差异</span><strong className={selectedTotal !== invoice?.totalFen ? "text-amber-700" : "text-emerald-700"}>{invoice?.totalFen == null ? "-" : money(selectedTotal - invoice.totalFen)}</strong></div></div>{differences.length > 0 && <div className="mt-4 border border-amber-200 bg-amber-50 p-3"><p className="text-xs font-medium text-amber-900">该组合不满足自动规则</p>{differences.map((value) => <p key={value} className="mt-1 text-xs text-amber-800">{value}</p>)}</div>}</aside>
    </div>
    <DialogFooter className="mx-0 mb-0 shrink-0 rounded-none"><Button variant="outline" disabled={submitting} onClick={() => onOpenChange(false)}>取消</Button>{differences.length > 0 ? <Button disabled={submitting} onClick={() => void submit(true)}>{submitting ? <LoaderCircle className="animate-spin" /> : <AlertTriangle />}接受差异并确认</Button> : <Button disabled={submitting || !selectedItems.length} onClick={() => void submit(false)}>{submitting ? <LoaderCircle className="animate-spin" /> : <Check />}检查并确认</Button>}</DialogFooter>
  </DialogContent>
  </Dialog>
}

function Summary({ label, value, tone }: { label: string; value: number; tone?: string }) { return <div className="border-r px-4 py-3 last:border-r-0"><p className="text-xs text-muted-foreground">{label}</p><p className={`mt-1 text-lg font-semibold tabular-nums ${tone ?? ""}`}>{value}</p></div> }
function Meta({ label, value }: { label: string; value: string }) { return <p className="min-w-0 truncate"><span className="text-muted-foreground">{label}：</span>{value}</p> }
function money(value: number | null, currency = "CNY") { if (value === null) return "-"; return `${currency === "CNY" ? "¥" : currency} ${(value / 100).toFixed(2)}` }
function StatusBadge({ status }: { status: MatchingStatus }) { const map: Record<MatchingStatus, { label: string; variant: "default" | "secondary" | "destructive" | "outline" }> = { suggested: { label: "待确认", variant: "default" }, conflict: { label: "冲突", variant: "destructive" }, unmatched: { label: "无法自动匹配", variant: "outline" }, ineligible: { label: "不参与", variant: "secondary" }, confirmed: { label: "已确认", variant: "outline" } }; return <Badge variant={map[status].variant}>{map[status].label}</Badge> }
