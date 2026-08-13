import { open } from "@tauri-apps/plugin-dialog"
import {
  AlertTriangle,
  Archive,
  ArchiveRestore,
  ArrowDown,
  ArrowUp,
  ChevronLeft,
  ChevronRight,
  Download,
  FileArchive,
  FolderKanban,
  LoaderCircle,
  Pencil,
  Plus,
  Search,
  Trash2,
  X,
} from "lucide-react"
import { useSetAtom } from "jotai"
import { useCallback, useEffect, useRef, useState } from "react"
import { toast } from "sonner"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
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
import { Input } from "@/components/ui/input"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"
import { api, errorCode, errorMessage } from "@/lib/api"
import { formatDateTime, formatMoney } from "@/lib/format"
import type { ProjectDetail, ProjectsPageData, ProjectSummary } from "@/lib/types"
import { cn } from "@/lib/utils"
import { workspaceRefreshAtom } from "@/lib/store"

const emptyPage: ProjectsPageData = { items: [], total: 0, page: 1, pageSize: 20 }

export function ProjectsPage({ refreshToken }: { refreshToken: number }) {
  const [data, setData] = useState<ProjectsPageData>(emptyPage)
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [detail, setDetail] = useState<ProjectDetail | null>(null)
  const [search, setSearch] = useState("")
  const [committedSearch, setCommittedSearch] = useState("")
  const [page, setPage] = useState(1)
  const [archivedView, setArchivedView] = useState(false)
  const [loading, setLoading] = useState(true)
  const [detailLoading, setDetailLoading] = useState(false)
  const [editing, setEditing] = useState<{ mode: "create" | "rename"; name: string } | null>(null)
  const [savingName, setSavingName] = useState(false)
  const [deleting, setDeleting] = useState(false)
  const [deleteOpen, setDeleteOpen] = useState(false)
  const [archiveOpen, setArchiveOpen] = useState(false)
  const [archiving, setArchiving] = useState(false)
  const [reordering, setReordering] = useState(false)
  const [exporting, setExporting] = useState(false)
  const [overwriteDirectory, setOverwriteDirectory] = useState<string | null>(null)
  const listRequest = useRef(0)
  const detailRequest = useRef(0)
  const refreshWorkspaces = useSetAtom(workspaceRefreshAtom)

  const loadProjects = useCallback(async () => {
    const request = ++listRequest.current
    setLoading(true)
    try {
      const result = await api.listProjects({ page, pageSize: 20, search: committedSearch || undefined, archived: archivedView })
      if (request !== listRequest.current) return
      const pages = Math.max(1, Math.ceil(result.total / result.pageSize))
      if (page > pages) setPage(pages)
      else {
        setData(result)
        setSelectedId((current) => {
          if (current && result.items.some((item) => item.id === current)) return current
          return result.items[0]?.id ?? null
        })
      }
    } catch (error) {
      toast.error(errorMessage(error))
    } finally {
      if (request === listRequest.current) setLoading(false)
    }
  }, [archivedView, committedSearch, page])

  const loadDetail = useCallback(async (id: string) => {
    const request = ++detailRequest.current
    setDetailLoading(true)
    try {
      const result = await api.getProject(id)
      if (request === detailRequest.current) setDetail(result)
    } catch (error) {
      toast.error(errorMessage(error))
    } finally {
      if (request === detailRequest.current) setDetailLoading(false)
    }
  }, [])

  useEffect(() => void loadProjects(), [loadProjects, refreshToken])
  useEffect(() => {
    if (selectedId) void loadDetail(selectedId)
    else setDetail(null)
  }, [loadDetail, selectedId, refreshToken])

  const saveName = async () => {
    if (!editing || savingName) return
    const name = editing.name.trim()
    if (!name) return
    setSavingName(true)
    try {
      const project = editing.mode === "create"
        ? await api.createProject(name)
        : await api.renameProject(selectedId!, name)
      setEditing(null)
      setSelectedId(project.id)
      setDetail(project)
      await loadProjects()
      toast.success(editing.mode === "create" ? "项目已创建" : "项目已重命名")
    } catch (error) {
      toast.error(errorMessage(error))
    } finally {
      setSavingName(false)
    }
  }

  const deleteProject = async () => {
    if (!selectedId || deleting) return
    setDeleting(true)
    try {
      await api.deleteProject(selectedId)
      setDeleteOpen(false)
      setSelectedId(null)
      setDetail(null)
      await loadProjects()
      refreshWorkspaces((value) => value + 1)
      toast.success("项目已删除，发票记录保持不变")
    } catch (error) {
      toast.error(errorMessage(error))
    } finally {
      setDeleting(false)
    }
  }

  const setArchived = async (archived: boolean) => {
    if (!selectedId || archiving) return
    setArchiving(true)
    try {
      if (archived) await api.archiveProject(selectedId)
      else await api.unarchiveProject(selectedId)
      setArchiveOpen(false)
      setSelectedId(null)
      setDetail(null)
      await loadProjects()
      refreshWorkspaces((value) => value + 1)
      toast.success(archived ? "项目已归档" : "项目已解除归档")
    } catch (error) {
      toast.error(errorMessage(error))
    } finally {
      setArchiving(false)
    }
  }

  const moveInvoice = async (index: number, direction: -1 | 1) => {
    if (!detail || detail.archivedAt || reordering) return
    const target = index + direction
    if (target < 0 || target >= detail.invoices.length) return
    const ids = detail.invoices.map((invoice) => invoice.id)
    ;[ids[index], ids[target]] = [ids[target], ids[index]]
    setReordering(true)
    try {
      const updated = await api.reorderProjectInvoices(detail.id, ids)
      setDetail(updated)
      await loadProjects()
    } catch (error) {
      toast.error(errorMessage(error))
      await loadDetail(detail.id)
    } finally {
      setReordering(false)
    }
  }

  const runExport = async (directory: string, overwrite: boolean) => {
    if (!detail) return
    setExporting(true)
    try {
      const result = await api.exportProject(detail.id, directory, overwrite)
      setOverwriteDirectory(null)
      toast.success(`已导出 ${result.invoiceCount} 张发票，实际金额 ${formatMoney(result.actualAmountFen)}`)
      await Promise.all([loadDetail(detail.id), loadProjects()])
    } catch (error) {
      if (errorCode(error) === "destination_exists" && !overwrite) setOverwriteDirectory(directory)
      else toast.error(errorMessage(error), { duration: 7000 })
    } finally {
      setExporting(false)
    }
  }

  const chooseExportDirectory = async () => {
    if (!detail || !detail.exportable || exporting) return
    const directory = await open({ directory: true, multiple: false, title: "选择项目材料包导出目录" })
    if (typeof directory === "string") await runExport(directory, false)
  }

  const pages = Math.max(1, Math.ceil(data.total / data.pageSize))

  return (
    <div className="flex h-screen min-h-0 flex-col bg-white">
      <header className="flex min-h-20 shrink-0 items-center justify-between border-b px-6">
        <div><h1 className="text-lg font-semibold">项目</h1><p className="mt-1 text-xs text-muted-foreground">整理发票并生成报销材料包</p></div>
        {!archivedView && <Button onClick={() => setEditing({ mode: "create", name: "" })}><Plus />新建项目</Button>}
      </header>
      <div className="grid min-h-0 flex-1 grid-cols-[minmax(280px,360px)_minmax(0,1fr)]">
        <aside className="flex min-h-0 flex-col border-r">
          <div className="grid grid-cols-2 border-b p-1">
            <Button variant={!archivedView ? "secondary" : "ghost"} size="sm" onClick={() => { setArchivedView(false); setPage(1); setSelectedId(null) }}>进行中</Button>
            <Button variant={archivedView ? "secondary" : "ghost"} size="sm" onClick={() => { setArchivedView(true); setPage(1); setSelectedId(null) }}>已归档</Button>
          </div>
          <form className="relative border-b p-3" onSubmit={(event) => { event.preventDefault(); setPage(1); setCommittedSearch(search.trim()) }}>
            <Search className="absolute left-6 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
            <Input value={search} onChange={(event) => setSearch(event.target.value)} className="pl-9 pr-9" placeholder="搜索项目" />
            {search && <Button type="button" variant="ghost" size="icon-sm" className="absolute right-4 top-1/2 -translate-y-1/2" onClick={() => { setSearch(""); setCommittedSearch(""); setPage(1) }}><X /></Button>}
          </form>
          <div className={cn("min-h-0 flex-1 overflow-y-auto", loading && "opacity-60")}>
            {data.items.map((project) => <ProjectListItem key={project.id} project={project} selected={project.id === selectedId} onSelect={() => setSelectedId(project.id)} />)}
            {!loading && data.items.length === 0 && <div className="flex h-72 flex-col items-center justify-center gap-3 text-muted-foreground"><FolderKanban className="size-8" /><p className="text-sm">暂无项目</p></div>}
          </div>
          <div className="flex h-12 shrink-0 items-center gap-2 border-t px-3"><span className="text-xs text-muted-foreground">共 {data.total} 个</span><div className="ml-auto flex items-center gap-2"><span className="text-xs tabular-nums">{data.page} / {pages}</span><Button variant="outline" size="icon-sm" disabled={page <= 1 || loading} onClick={() => setPage((value) => value - 1)}><ChevronLeft /></Button><Button variant="outline" size="icon-sm" disabled={page >= pages || loading} onClick={() => setPage((value) => value + 1)}><ChevronRight /></Button></div></div>
        </aside>
        <main className="flex min-h-0 min-w-0 flex-col">
          {detail ? <>
            <div className="flex min-h-20 shrink-0 items-center gap-4 border-b px-6">
              <div className="min-w-0"><div className="flex items-center gap-2"><h2 className="truncate text-base font-semibold">{detail.name}</h2>{detail.archivedAt ? <Badge variant="secondary">已归档</Badge> : <StatusBadge project={detail} />}</div><p className="mt-1 text-xs text-muted-foreground">{detail.confirmedCount} / {detail.invoiceCount} 张已确认 · 实际金额 {formatMoney(detail.actualAmountFen)}{detail.archivedAt ? ` · 归档于 ${formatDateTime(detail.archivedAt)}` : ""}</p></div>
              <div className="ml-auto flex items-center gap-2">
                {!detail.archivedAt && <Tooltip><TooltipTrigger render={<Button variant="outline" size="icon" aria-label="重命名项目" onClick={() => setEditing({ mode: "rename", name: detail.name })} />}><Pencil /></TooltipTrigger><TooltipContent>重命名项目</TooltipContent></Tooltip>}
                {detail.archivedAt ? <Tooltip><TooltipTrigger render={<Button variant="outline" size="icon" aria-label="解除归档" disabled={archiving} onClick={() => void setArchived(false)} />}><ArchiveRestore /></TooltipTrigger><TooltipContent>解除归档</TooltipContent></Tooltip> : <Tooltip><TooltipTrigger render={<Button variant="outline" size="icon" aria-label="归档项目" onClick={() => setArchiveOpen(true)} />}><Archive /></TooltipTrigger><TooltipContent>归档项目</TooltipContent></Tooltip>}
                <Tooltip><TooltipTrigger render={<Button variant="outline" size="icon" aria-label="删除项目" onClick={() => setDeleteOpen(true)} />}><Trash2 /></TooltipTrigger><TooltipContent>删除项目</TooltipContent></Tooltip>
                <Button disabled={!detail.exportable || exporting} onClick={() => void chooseExportDirectory()}>{exporting ? <LoaderCircle className="animate-spin" /> : <Download />}导出材料包</Button>
              </div>
            </div>
            {detail.blockingReason && <div className="shrink-0 border-b px-6 py-3"><Alert variant={detail.exportable ? "default" : "destructive"}><AlertTriangle /><AlertTitle>当前不可导出</AlertTitle><AlertDescription>{detail.blockingReason}</AlertDescription></Alert></div>}
            <div className={cn("min-h-0 flex-1 overflow-auto", detailLoading && "opacity-60")}>
              <Table><TableHeader className="sticky top-0 z-10 bg-zinc-50"><TableRow><TableHead className="w-20">顺序</TableHead><TableHead>发票</TableHead><TableHead>销售方</TableHead><TableHead>订单与商品</TableHead><TableHead>状态</TableHead><TableHead className="text-right">发票金额</TableHead><TableHead className="text-right">订单金额</TableHead><TableHead className="text-right">实际金额</TableHead></TableRow></TableHeader><TableBody>{detail.invoices.map((invoice, index) => <TableRow key={invoice.id} className="h-16"><TableCell><div className="flex items-center gap-1"><span className="w-6 text-xs tabular-nums text-muted-foreground">{index + 1}</span><Tooltip><TooltipTrigger render={<Button variant="ghost" size="icon-xs" aria-label={`上移 ${invoice.invoiceNumber ?? invoice.id}`} disabled={Boolean(detail.archivedAt) || index === 0 || reordering} onClick={() => void moveInvoice(index, -1)} />}><ArrowUp /></TooltipTrigger><TooltipContent>上移</TooltipContent></Tooltip><Tooltip><TooltipTrigger render={<Button variant="ghost" size="icon-xs" aria-label={`下移 ${invoice.invoiceNumber ?? invoice.id}`} disabled={Boolean(detail.archivedAt) || index === detail.invoices.length - 1 || reordering} onClick={() => void moveInvoice(index, 1)} />}><ArrowDown /></TooltipTrigger><TooltipContent>下移</TooltipContent></Tooltip></div></TableCell><TableCell><p className="font-medium">{invoice.invoiceNumber ?? "未识别号码"}</p><p className="mt-1 text-xs text-muted-foreground">{invoice.typeDescription} · {invoice.invoiceDate ?? "日期缺失"}</p></TableCell><TableCell className="max-w-44 truncate">{invoice.sellerName ?? "-"}</TableCell><TableCell className="max-w-72"><p className="truncate">{invoice.orders.map((order) => order.externalOrderNumber ?? order.id).join("、") || "-"}</p><p className="mt-1 truncate text-xs text-muted-foreground">{invoice.orders.flatMap((order) => order.productNames).join("、") || "暂无商品摘要"}</p></TableCell><TableCell>{invoice.blockingReason ? <Tooltip><TooltipTrigger render={<Badge variant="destructive" />}>异常</TooltipTrigger><TooltipContent>{invoice.blockingReason}</TooltipContent></Tooltip> : <Badge variant="outline">已确认</Badge>}</TableCell><MoneyCell value={invoice.invoiceAmountFen} /><MoneyCell value={invoice.orderAmountFen} /><MoneyCell value={invoice.actualAmountFen} strong /></TableRow>)}{detail.invoices.length === 0 && <TableRow><TableCell colSpan={8} className="h-72 text-center text-sm text-muted-foreground">请在发票台账中选择发票并分配到此项目</TableCell></TableRow>}</TableBody></Table>
            </div>
          </> : <div className="flex flex-1 flex-col items-center justify-center gap-3 text-muted-foreground"><FileArchive className="size-10" /><p className="text-sm">选择或创建一个项目</p></div>}
        </main>
      </div>

      <Dialog open={Boolean(editing)} onOpenChange={(open) => !open && !savingName && setEditing(null)}><DialogContent><DialogHeader><DialogTitle>{editing?.mode === "create" ? "新建项目" : "重命名项目"}</DialogTitle><DialogDescription>项目名称用于列表和导出材料包文件名，不能与已有项目重复。</DialogDescription></DialogHeader><Input autoFocus maxLength={100} value={editing?.name ?? ""} onChange={(event) => setEditing((current) => current && { ...current, name: event.target.value })} onKeyDown={(event) => { if (event.key === "Enter") void saveName() }} placeholder="输入项目名称" /><DialogFooter><Button variant="outline" disabled={savingName} onClick={() => setEditing(null)}>取消</Button><Button disabled={savingName || !editing?.name.trim()} onClick={() => void saveName()}>{savingName && <LoaderCircle className="animate-spin" />}保存</Button></DialogFooter></DialogContent></Dialog>
      <Dialog open={deleteOpen} onOpenChange={(open) => !deleting && setDeleteOpen(open)}><DialogContent><DialogHeader><DialogTitle>删除项目？</DialogTitle><DialogDescription>将删除项目“{detail?.name}”并解除 {detail?.invoiceCount ?? 0} 张发票的项目归属。发票、订单、匹配关系和原文件都会保留。</DialogDescription></DialogHeader><DialogFooter><Button variant="outline" disabled={deleting} onClick={() => setDeleteOpen(false)}>取消</Button><Button variant="destructive" disabled={deleting} onClick={() => void deleteProject()}>{deleting ? <LoaderCircle className="animate-spin" /> : <Trash2 />}确认删除</Button></DialogFooter></DialogContent></Dialog>
      <Dialog open={archiveOpen} onOpenChange={(open) => !archiving && setArchiveOpen(open)}><DialogContent><DialogHeader><DialogTitle>归档项目？</DialogTitle><DialogDescription>归档后，项目关联的订单和发票将退出日常工作台。项目、匹配关系和文件都会保留，仍可导出并随时解除归档。</DialogDescription></DialogHeader><DialogFooter><Button variant="outline" disabled={archiving} onClick={() => setArchiveOpen(false)}>取消</Button><Button disabled={archiving} onClick={() => void setArchived(true)}>{archiving ? <LoaderCircle className="animate-spin" /> : <Archive />}确认归档</Button></DialogFooter></DialogContent></Dialog>
      <Dialog open={Boolean(overwriteDirectory)} onOpenChange={(open) => !open && !exporting && setOverwriteDirectory(null)}><DialogContent><DialogHeader><DialogTitle>覆盖已有材料包？</DialogTitle><DialogDescription>所选目录中已有同名 ZIP。确认后将安全替换旧文件；生成失败时保留旧文件。</DialogDescription></DialogHeader><DialogFooter><Button variant="outline" disabled={exporting} onClick={() => setOverwriteDirectory(null)}>取消</Button><Button disabled={exporting} onClick={() => overwriteDirectory && void runExport(overwriteDirectory, true)}>{exporting ? <LoaderCircle className="animate-spin" /> : <Download />}覆盖导出</Button></DialogFooter></DialogContent></Dialog>
    </div>
  )
}

function ProjectListItem({ project, selected, onSelect }: { project: ProjectSummary; selected: boolean; onSelect: () => void }) {
  return <button type="button" className={cn("flex w-full flex-col gap-2 border-b px-4 py-3 text-left hover:bg-zinc-50", selected && "bg-zinc-100")} onClick={onSelect}><div className="flex w-full items-center gap-2"><span className="min-w-0 flex-1 truncate text-sm font-medium">{project.name}</span><StatusBadge project={project} /></div><div className="flex items-center gap-3 text-xs text-muted-foreground"><span>{project.confirmedCount}/{project.invoiceCount} 张确认</span><span className="tabular-nums">{formatMoney(project.actualAmountFen)}</span><span className="ml-auto">{formatDateTime(project.updatedAt)}</span></div></button>
}

function StatusBadge({ project }: { project: Pick<ProjectSummary, "exportable" | "invoiceCount" | "unconfirmedCount" | "blockingReason"> }) {
  if (project.exportable) return <Badge variant="outline">可导出</Badge>
  if (project.invoiceCount === 0) return <Badge variant="secondary">空项目</Badge>
  if (project.unconfirmedCount > 0) return <Badge variant="secondary">待确认</Badge>
  return <Badge variant="destructive">异常</Badge>
}

function MoneyCell({ value, strong = false }: { value: number | null; strong?: boolean }) {
  return <TableCell className={cn("text-right tabular-nums", strong && "font-medium")}>{value === null ? "-" : formatMoney(value)}</TableCell>
}
