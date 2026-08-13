import { open } from "@tauri-apps/plugin-dialog"
import {
  CalendarDays,
  ChevronLeft,
  ChevronRight,
  FileSearch,
  Filter,
  FolderInput,
  FolderMinus,
  LoaderCircle,
  ReceiptText,
  Search,
  Trash2,
  Upload,
  X,
} from "lucide-react"
import { lazy, Suspense, useCallback, useEffect, useRef, useState } from "react"
import { toast } from "sonner"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { api, errorMessage } from "@/lib/api"
import { formatDateTime } from "@/lib/format"
import type { InvoiceBatchDetail, InvoiceBatchPreview, InvoiceListItem, InvoiceProjectConflict, InvoicesPage as PageData, InvoiceSortBy, ProjectSummary, SortDirection } from "@/lib/types"

const emptyPage: PageData = { items: [], total: 0, page: 1, pageSize: 20 }
const InvoiceSheet = lazy(() => import("@/components/invoice-sheet"))
const pageSizes = [20, 50, 100]

const invoiceTypes = [
  ["", "自动识别全部"], ["3", "增值税发票"], ["16", "全电发票"], ["0", "出租车发票"],
  ["1", "定额发票"], ["2", "火车票"], ["5", "机票行程单"], ["8", "通用机打发票"],
  ["9", "汽车票"], ["10", "轮船票"], ["11", "增值税卷票"], ["12", "购车发票"],
  ["13", "过路过桥费"], ["15", "非税发票"], ["17", "医疗发票"], ["18", "完税凭证"],
  ["19", "海关缴款书"], ["20", "银行回单"], ["21", "网约车行程单"], ["22", "海关报关单"],
  ["23", "海外发票"], ["24", "购物小票"], ["25", "销货清单"], ["-1", "其他发票"],
] as const

const ledgerSubtypes = [
  ["", "全部票种"], ["VatSpecialInvoice", "增值税专用发票"], ["VatCommonInvoice", "增值税普通发票"],
  ["VatElectronicCommonInvoice", "增值税电子普票"], ["VatElectronicSpecialInvoice", "增值税电子专票"],
  ["VatElectronicInvoiceBlockchain", "区块链电子发票"], ["VatElectronicInvoiceToll", "通行费电子普票"],
  ["VatSalesList", "增值税销货清单"],
  ["VatElectronicInvoiceFull", "电子发票（普通）"], ["VatElectronicSpecialInvoiceFull", "电子发票（专用）"],
  ["ElectronicTrainTicketFull", "铁路电子客票"], ["ElectronicFlightTicketFull", "电子机票行程单"],
  ["MotorVehicleSaleInvoice", "机动车销售发票"], ["UsedCarPurchaseInvoice", "二手车销售发票"],
  ["MotorVehicleSaleInvoiceElectronic", "机动车销售发票（电子）"], ["UsedCarPurchaseInvoiceElectronic", "二手车销售发票（电子）"],
  ["VatInvoiceRoll", "增值税卷票"], ["TaxiTicket", "出租车发票"], ["QuotaInvoice", "定额发票"],
  ["TrainTicket", "火车票"], ["AirTransport", "机票行程单"],
  ["MachinePrintedInvoice", "通用机打发票"], ["BusInvoice", "汽车票"], ["ShippingInvoice", "轮船票"],
  ["NonTaxIncomeGeneralBill", "非税收入通用票据"], ["NonTaxIncomeElectronicBill", "非税电子缴款书"],
  ["TollInvoice", "过路过桥费发票"], ["MedicalOutpatientInvoice", "医疗门诊票据"],
  ["MedicalHospitalizedInvoice", "医疗住院票据"], ["TaxPayment", "完税凭证"],
  ["CustomsPaymentReceipt", "海关缴款书"], ["BankSlip", "银行回单"],
  ["OnlineTaxiItinerary", "网约车行程单"], ["CustomsDeclaration", "海关报关单"],
  ["OverseasInvoice", "海外发票"], ["ShoppingReceipt", "购物小票"], ["SaleInventory", "销货清单"],
  ["ElectronicTollSummary", "通行费电子票据汇总单"], ["OtherInvoice", "其他发票"],
] as const

export function InvoicesPage({ onBatchCreated, refreshToken, projectRefreshToken = 0, onProjectsChanged = () => undefined }: { onBatchCreated: (batch: InvoiceBatchDetail) => void; refreshToken: number; projectRefreshToken?: number; onProjectsChanged?: () => void }) {
  const [data, setData] = useState<PageData>(emptyPage)
  const [page, setPage] = useState(1)
  const [pageSize, setPageSize] = useState(20)
  const [search, setSearch] = useState("")
  const [committedSearch, setCommittedSearch] = useState("")
  const [dateFrom, setDateFrom] = useState("")
  const [dateTo, setDateTo] = useState("")
  const [duplicateOnly, setDuplicateOnly] = useState(false)
  const [projectFilter, setProjectFilter] = useState("all")
  const [projects, setProjects] = useState<ProjectSummary[]>([])
  const [subtype, setSubtype] = useState("")
  const [sortBy, setSortBy] = useState<InvoiceSortBy>("created_at")
  const [sortDirection, setSortDirection] = useState<SortDirection>("desc")
  const [loading, setLoading] = useState(true)
  const [selectedIds, setSelectedIds] = useState<string[]>([])
  const [selectedInvoice, setSelectedInvoice] = useState<string | null>(null)
  const [bulkDeleteOpen, setBulkDeleteOpen] = useState(false)
  const [bulkDeleting, setBulkDeleting] = useState(false)
  const [assignOpen, setAssignOpen] = useState(false)
  const [assignProjectId, setAssignProjectId] = useState("")
  const [assignConflicts, setAssignConflicts] = useState<InvoiceProjectConflict[]>([])
  const [assigning, setAssigning] = useState(false)
  const [removeProjectOpen, setRemoveProjectOpen] = useState(false)
  const [preflight, setPreflight] = useState<InvoiceBatchPreview | null>(null)
  const [preparing, setPreparing] = useState(false)
  const [creating, setCreating] = useState(false)
  const [forcedType, setForcedType] = useState("")
  const [localRefresh, setLocalRefresh] = useState(0)
  const requestRef = useRef(0)

  const clearSelection = useCallback(() => { setSelectedIds([]); setBulkDeleteOpen(false) }, [])
  const load = useCallback(async () => {
    const request = ++requestRef.current
    setLoading(true)
    try {
      const result = await api.listInvoices({ page, pageSize, search: committedSearch || undefined, subtype: subtype || undefined, dateFrom: dateFrom || undefined, dateTo: dateTo || undefined, duplicateOnly, projectId: projectFilter !== "all" && projectFilter !== "unassigned" ? projectFilter : undefined, unassignedOnly: projectFilter === "unassigned", sortBy, sortDirection })
      if (request !== requestRef.current) return
      const pages = Math.max(1, Math.ceil(result.total / result.pageSize))
      if (page > pages) { clearSelection(); setPage(pages) } else {
        setData(result)
        const visible = new Set(result.items.map((item) => item.id))
        setSelectedIds((current) => current.filter((id) => visible.has(id)))
      }
    } catch (error) { toast.error(errorMessage(error)) } finally { if (request === requestRef.current) setLoading(false) }
  }, [clearSelection, committedSearch, dateFrom, dateTo, duplicateOnly, page, pageSize, projectFilter, sortBy, sortDirection, subtype])
  useEffect(() => void load(), [load, localRefresh, refreshToken])
  useEffect(() => {
    api.listProjects({ page: 1, pageSize: 100 }).then((result) => setProjects(result.items)).catch((error) => toast.error(errorMessage(error)))
  }, [projectRefreshToken, localRefresh])

  const importFiles = async () => {
    const selected = await open({ multiple: true, directory: false, title: "选择发票文件", filters: [{ name: "发票", extensions: ["png", "jpg", "jpeg", "pdf"] }] })
    if (!selected) return
    const paths = Array.isArray(selected) ? selected : [selected]
    if (paths.length > 500) { toast.error("单个批次最多选择 500 个文件"); return }
    setPreparing(true)
    try { setPreflight(await api.prepareInvoiceBatch(paths, forcedType === "" ? null : Number(forcedType))) }
    catch (error) { toast.error(errorMessage(error), { duration: 6000 }) }
    finally { setPreparing(false) }
  }

  const confirmImport = async () => {
    if (!preflight || creating) return
    setCreating(true)
    try {
      const batch = await api.createInvoiceBatch(preflight.preflightToken)
      onBatchCreated(batch)
      setPreflight(null)
      toast.success(`已创建 ${batch.counts.total} 个发票任务`)
    } catch (error) { toast.error(errorMessage(error), { duration: 7000 }) }
    finally { setCreating(false) }
  }

  const deleteSelected = async () => {
    if (!selectedIds.length || bulkDeleting) return
    setBulkDeleting(true)
    const requested = [...selectedIds]
    try {
      const result = await api.deleteInvoices(requested)
      const deleted = new Set(result.results.filter((item) => item.deleted).map((item) => item.id))
      const failures = result.results.filter((item) => !item.deleted)
      setSelectedIds((current) => current.filter((id) => !deleted.has(id)))
      setBulkDeleteOpen(false)
      if (deleted.size) toast.success(`已删除 ${deleted.size} 张发票`)
      if (failures.length) {
        const labels = failures.slice(0, 3).map((failure) => data.items.find((item) => item.id === failure.id)?.invoiceNumber ?? failure.id)
        toast.error(`${failures.length} 张发票删除失败：${labels.join("、")}${failures.length > 3 ? " 等" : ""}`)
      }
      setLocalRefresh((value) => value + 1)
    } catch (error) { toast.error(errorMessage(error)) } finally { setBulkDeleting(false) }
  }

  const assignSelected = async (acceptReassignment: boolean) => {
    if (!selectedIds.length || !assignProjectId || assigning) return
    setAssigning(true)
    try {
      const result = await api.assignInvoicesToProject(selectedIds, assignProjectId, acceptReassignment)
      if (!result.assigned) { setAssignConflicts(result.conflicts); return }
      const count = selectedIds.length
      setAssignOpen(false); setAssignConflicts([]); setSelectedIds([])
      setLocalRefresh((value) => value + 1); onProjectsChanged()
      toast.success(`已将 ${count} 张发票分配到项目`)
    } catch (error) { toast.error(errorMessage(error)) }
    finally { setAssigning(false) }
  }

  const removeSelectedFromProjects = async () => {
    if (!selectedIds.length || assigning) return
    setAssigning(true)
    try {
      await api.removeInvoicesFromProjects(selectedIds)
      setRemoveProjectOpen(false); setSelectedIds([])
      setLocalRefresh((value) => value + 1); onProjectsChanged()
      toast.success("已移出项目，发票和匹配关系保持不变")
    } catch (error) { toast.error(errorMessage(error)) }
    finally { setAssigning(false) }
  }

  const allSelected = data.items.length > 0 && data.items.every((item) => selectedIds.includes(item.id))
  const someSelected = data.items.some((item) => selectedIds.includes(item.id))
  const pages = Math.max(1, Math.ceil(data.total / data.pageSize))

  return (
    <div className="flex h-screen min-h-0 flex-col bg-white">
      <header className="flex min-h-20 shrink-0 items-center justify-between border-b px-6">
        <div><h1 className="text-lg font-semibold">发票台账</h1><p className="mt-1 text-xs text-muted-foreground">识别、复核并维护本机发票记录</p></div>
        <div className="flex items-center gap-2">
          <Select value={forcedType} onValueChange={(value) => setForcedType(value ?? "")}><SelectTrigger aria-label="强制识别票种" className="w-40"><SelectValue placeholder="自动识别全部" /></SelectTrigger><SelectContent>{invoiceTypes.map(([value, label]) => <SelectItem key={value || "auto"} value={value}>{label}</SelectItem>)}</SelectContent></Select>
          <Button disabled={preparing} onClick={() => void importFiles()}>{preparing ? <LoaderCircle className="animate-spin" /> : <Upload />}导入发票</Button>
        </div>
      </header>
      <div className="flex min-h-14 shrink-0 items-center gap-3 overflow-x-auto border-b px-6">
        <form className="relative min-w-72 max-w-sm flex-1" onSubmit={(event) => { event.preventDefault(); clearSelection(); setPage(1); setCommittedSearch(search.trim()) }}>
          <Search className="absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" /><Input value={search} onChange={(event) => setSearch(event.target.value)} className="pl-9 pr-9" placeholder="搜索代码、号码、购销方或税号" />{search && <Button type="button" variant="ghost" size="icon-sm" className="absolute right-1 top-1/2 -translate-y-1/2" onClick={() => { setSearch(""); setCommittedSearch(""); clearSelection(); setPage(1) }}><X /></Button>}
        </form>
        <CalendarDays className="size-4 text-muted-foreground" /><Input type="date" aria-label="发票日期起" className="w-36" value={dateFrom} onChange={(event) => { clearSelection(); setDateFrom(event.target.value); setPage(1) }} /><span className="text-xs text-muted-foreground">至</span><Input type="date" aria-label="发票日期止" className="w-36" value={dateTo} onChange={(event) => { clearSelection(); setDateTo(event.target.value); setPage(1) }} />
        <Select value={subtype} onValueChange={(value) => { clearSelection(); setSubtype(value ?? ""); setPage(1) }}><SelectTrigger aria-label="票种筛选" className="w-40"><SelectValue placeholder="全部票种" /></SelectTrigger><SelectContent>{ledgerSubtypes.map(([value, label]) => <SelectItem key={value || "all"} value={value}>{label}</SelectItem>)}</SelectContent></Select>
        <Select value={projectFilter} onValueChange={(value) => { clearSelection(); setProjectFilter(value ?? "all"); setPage(1) }}><SelectTrigger aria-label="项目筛选" className="w-40"><SelectValue /></SelectTrigger><SelectContent><SelectItem value="all">全部项目归属</SelectItem><SelectItem value="unassigned">未分配项目</SelectItem>{projects.map((project) => <SelectItem key={project.id} value={project.id}>{project.name}</SelectItem>)}</SelectContent></Select>
        <label className="flex h-9 shrink-0 cursor-pointer items-center gap-2 border px-3 text-xs"><Filter className="size-3.5" /><Checkbox checked={duplicateOnly} onCheckedChange={(checked) => { clearSelection(); setDuplicateOnly(checked === true); setPage(1) }} />仅可能重复</label>
        <Select value={`${sortBy}:${sortDirection}`} onValueChange={(value) => { if (!value) return; const [sort, direction] = value.split(":") as [InvoiceSortBy, SortDirection]; clearSelection(); setSortBy(sort); setSortDirection(direction); setPage(1) }}><SelectTrigger className="w-36"><SelectValue /></SelectTrigger><SelectContent><SelectItem value="created_at:desc">最新入库</SelectItem><SelectItem value="invoice_date:desc">开票日期降序</SelectItem><SelectItem value="invoice_date:asc">开票日期升序</SelectItem><SelectItem value="total_amount:desc">金额从高到低</SelectItem><SelectItem value="total_amount:asc">金额从低到高</SelectItem></SelectContent></Select>
      </div>
      {selectedIds.length > 0 && <div className="flex h-11 shrink-0 items-center gap-3 border-b bg-zinc-50 px-6"><span className="text-xs">已选择 {selectedIds.length} 张</span><Button variant="outline" size="sm" onClick={() => { setAssignProjectId(""); setAssignConflicts([]); setAssignOpen(true) }}><FolderInput />分配项目</Button><Button variant="outline" size="sm" onClick={() => setRemoveProjectOpen(true)}><FolderMinus />移出项目</Button><Button variant="destructive" size="sm" onClick={() => setBulkDeleteOpen(true)}><Trash2 />删除</Button></div>}
      <div className="min-h-0 flex-1 overflow-auto">
        <Table><TableHeader className="sticky top-0 z-10 bg-zinc-50"><TableRow><TableHead className="w-12"><Checkbox checked={allSelected} indeterminate={!allSelected && someSelected} aria-label="选择当前页" onCheckedChange={(checked) => setSelectedIds(checked === true ? data.items.map((item) => item.id) : [])} /></TableHead><TableHead>发票号码</TableHead><TableHead>票种</TableHead><TableHead>所属项目</TableHead><TableHead>购买方</TableHead><TableHead>销售方</TableHead><TableHead>开票日期</TableHead><TableHead className="text-right">价税合计</TableHead><TableHead>入库时间</TableHead></TableRow></TableHeader>
          <TableBody className={loading ? "opacity-50" : undefined}>{data.items.map((item) => <InvoiceRow key={item.id} item={item} checked={selectedIds.includes(item.id)} onChecked={(checked) => setSelectedIds((current) => checked ? [...current, item.id] : current.filter((id) => id !== item.id))} onOpen={() => setSelectedInvoice(item.id)} />)}{!loading && !data.items.length && <TableRow><TableCell colSpan={9} className="h-72"><div className="flex flex-col items-center gap-3 text-muted-foreground"><ReceiptText className="size-8" /><p className="text-sm">暂无发票</p></div></TableCell></TableRow>}</TableBody>
        </Table>
      </div>
      <div className="flex h-14 shrink-0 items-center gap-3 border-t px-6"><span className="text-xs tabular-nums text-muted-foreground">共 {data.total} 张</span><div className="ml-auto flex items-center gap-2"><Select value={String(pageSize)} onValueChange={(value) => { if (!value) return; clearSelection(); setPageSize(Number(value)); setPage(1) }}><SelectTrigger size="sm" className="w-20"><SelectValue /></SelectTrigger><SelectContent>{pageSizes.map((size) => <SelectItem key={size} value={String(size)}>{size} 条</SelectItem>)}</SelectContent></Select><span className="min-w-14 text-center text-xs tabular-nums">{data.page} / {pages}</span><Button variant="outline" size="icon-sm" disabled={page <= 1 || loading} onClick={() => { clearSelection(); setPage((value) => value - 1) }}><ChevronLeft /></Button><Button variant="outline" size="icon-sm" disabled={page >= pages || loading} onClick={() => { clearSelection(); setPage((value) => value + 1) }}><ChevronRight /></Button></div></div>

      <Dialog open={Boolean(preflight)} onOpenChange={(open) => !open && !creating && setPreflight(null)}><DialogContent className="max-w-2xl"><DialogHeader><DialogTitle>确认发票识别</DialogTitle><DialogDescription>确认后将复制有效文件并调用腾讯云 OCR，识别请求可能产生费用。</DialogDescription></DialogHeader>{preflight && <div className="space-y-4"><div className="grid grid-cols-5 border"><Metric label="所选文件" value={preflight.selectedCount} /><Metric label="有效" value={preflight.validCount} /><Metric label="重复" value={preflight.duplicateCount} /><Metric label="无效" value={preflight.invalidCount} /><Metric label="预计请求" value={preflight.predictedRequests} /></div>{preflight.totalPdfPages > 0 && <p className="text-xs text-muted-foreground">PDF 合计 {preflight.totalPdfPages} 页，仅支持每份前 30 页。</p>}<div className="max-h-52 overflow-y-auto border">{preflight.files.map((file) => <div key={file.path} className="flex items-center gap-3 border-b px-3 py-2 text-xs last:border-0"><FileSearch className="size-4 shrink-0" /><div className="min-w-0 flex-1"><p className="truncate">{file.originalFileName}</p>{file.message && <p className="truncate text-muted-foreground">{file.message}</p>}</div><Badge variant={file.status === "valid" ? "secondary" : file.status === "duplicate" ? "outline" : "destructive"}>{file.status === "valid" ? "待识别" : file.status === "duplicate" ? "重复" : "无效"}</Badge></div>)}</div></div>}<DialogFooter><Button variant="outline" disabled={creating} onClick={() => setPreflight(null)}>取消</Button><Button disabled={creating} onClick={() => void confirmImport()}>{creating ? <LoaderCircle className="animate-spin" /> : <Upload />}确认并创建任务</Button></DialogFooter></DialogContent></Dialog>
      <Dialog open={bulkDeleteOpen} onOpenChange={(open) => !bulkDeleting && setBulkDeleteOpen(open)}><DialogContent><DialogHeader><DialogTitle>删除选中的发票？</DialogTitle><DialogDescription>将删除 {selectedIds.length} 张发票、拆分预览和不再引用的源文件。</DialogDescription></DialogHeader><DialogFooter><Button variant="outline" disabled={bulkDeleting} onClick={() => setBulkDeleteOpen(false)}>取消</Button><Button variant="destructive" disabled={bulkDeleting} onClick={() => void deleteSelected()}>{bulkDeleting ? <LoaderCircle className="animate-spin" /> : <Trash2 />}确认删除</Button></DialogFooter></DialogContent></Dialog>
      <Dialog open={assignOpen} onOpenChange={(open) => { if (!assigning && !open) { setAssignOpen(false); setAssignConflicts([]) } }}><DialogContent><DialogHeader><DialogTitle>{assignConflicts.length ? "确认改派发票？" : "分配到项目"}</DialogTitle><DialogDescription>{assignConflicts.length ? `${assignConflicts.length} 张发票已属于其他项目。确认后将从原项目原子改派，发票和匹配关系保持不变。` : `将选中的 ${selectedIds.length} 张发票分配到一个项目，未确认发票也可以提前归类。`}</DialogDescription></DialogHeader>{assignConflicts.length ? <div className="max-h-48 overflow-y-auto border">{assignConflicts.map((conflict) => <div key={conflict.invoiceId} className="border-b px-3 py-2 text-xs last:border-0"><span className="font-medium">{data.items.find((item) => item.id === conflict.invoiceId)?.invoiceNumber ?? conflict.invoiceId}</span><span className="ml-2 text-muted-foreground">来自 {conflict.sourceProjectName}</span></div>)}</div> : <><Select value={assignProjectId} onValueChange={(value) => setAssignProjectId(value ?? "")}><SelectTrigger aria-label="目标项目"><SelectValue placeholder="选择项目" /></SelectTrigger><SelectContent>{projects.map((project) => <SelectItem key={project.id} value={project.id}>{project.name}</SelectItem>)}</SelectContent></Select>{!projects.length && <p className="text-xs text-muted-foreground">暂无项目，请先在项目页创建。</p>}</>}<DialogFooter><Button variant="outline" disabled={assigning} onClick={() => { setAssignOpen(false); setAssignConflicts([]) }}>取消</Button><Button disabled={assigning || !assignProjectId} onClick={() => void assignSelected(assignConflicts.length > 0)}>{assigning ? <LoaderCircle className="animate-spin" /> : <FolderInput />}{assignConflicts.length ? "确认改派" : "确认分配"}</Button></DialogFooter></DialogContent></Dialog>
      <Dialog open={removeProjectOpen} onOpenChange={(open) => !assigning && setRemoveProjectOpen(open)}><DialogContent><DialogHeader><DialogTitle>将发票移出项目？</DialogTitle><DialogDescription>将解除选中的 {selectedIds.length} 张发票的项目归属。发票、订单、匹配关系和原文件都会保留。</DialogDescription></DialogHeader><DialogFooter><Button variant="outline" disabled={assigning} onClick={() => setRemoveProjectOpen(false)}>取消</Button><Button disabled={assigning} onClick={() => void removeSelectedFromProjects()}>{assigning ? <LoaderCircle className="animate-spin" /> : <FolderMinus />}确认移出</Button></DialogFooter></DialogContent></Dialog>
      <Suspense fallback={null}><InvoiceSheet invoiceId={selectedInvoice} onOpenChange={(open) => !open && setSelectedInvoice(null)} onChanged={() => setLocalRefresh((value) => value + 1)} /></Suspense>
    </div>
  )
}

function InvoiceRow({ item, checked, onChecked, onOpen }: { item: InvoiceListItem; checked: boolean; onChecked: (checked: boolean) => void; onOpen: () => void }) { return <TableRow data-state={checked ? "selected" : undefined} className="h-14 cursor-pointer" onClick={onOpen}><TableCell onClick={(event) => event.stopPropagation()}><Checkbox checked={checked} aria-label={`选择发票 ${item.invoiceNumber ?? item.id}`} onCheckedChange={(value) => onChecked(value === true)} /></TableCell><TableCell><p className="font-medium">{item.invoiceNumber ?? "未识别号码"}</p>{item.invoiceCode && <p className="text-xs text-muted-foreground">{item.invoiceCode}</p>}</TableCell><TableCell><Badge variant="outline">{item.subtypeDescription}</Badge></TableCell><TableCell>{item.projectName ? <Badge variant="secondary">{item.projectName}</Badge> : <span className="text-xs text-muted-foreground">未分配</span>}</TableCell><TableCell className="max-w-48 truncate">{item.buyerName ?? "-"}</TableCell><TableCell className="max-w-48 truncate">{item.sellerName ?? "-"}</TableCell><TableCell>{item.invoiceDate ?? "-"}</TableCell><TableCell className="text-right tabular-nums">{item.totalFen === null ? "-" : `${item.currency === "CNY" ? "¥" : item.currency} ${(item.totalFen / 100).toFixed(2)}`}</TableCell><TableCell className="text-xs text-muted-foreground">{formatDateTime(item.createdAt)}</TableCell></TableRow> }
function Metric({ label, value }: { label: string; value: number }) { return <div className="p-3 text-center"><p className="text-lg font-semibold tabular-nums">{value}</p><p className="text-xs text-muted-foreground">{label}</p></div> }
