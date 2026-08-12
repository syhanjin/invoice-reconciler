import { zodResolver } from "@hookform/resolvers/zod"
import { convertFileSrc } from "@tauri-apps/api/core"
import { FileJson2, LoaderCircle, Save, Trash2 } from "lucide-react"
import { useEffect, useState } from "react"
import { useForm } from "react-hook-form"
import { toast } from "sonner"
import { z } from "zod"

import { PdfPreview } from "@/components/pdf-preview"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Separator } from "@/components/ui/separator"
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from "@/components/ui/sheet"
import { Skeleton } from "@/components/ui/skeleton"
import { Textarea } from "@/components/ui/textarea"
import { api, errorMessage } from "@/lib/api"
import { parseYuanToFen } from "@/lib/format"
import type { InvoiceDetail } from "@/lib/types"

const schema = z.object({
  invoiceCode: z.string().max(100), invoiceNumber: z.string().max(100), invoiceDate: z.string().max(32),
  buyerName: z.string().max(300), buyerTaxId: z.string().max(100), sellerName: z.string().max(300), sellerTaxId: z.string().max(100),
  amount: moneyField(), tax: moneyField(), total: moneyField(), currency: z.string().trim().min(3).max(3),
})
function moneyField() { return z.string().refine((value) => value === "" || /^\d+(?:\.\d{1,2})?$/.test(value), "请输入有效金额") }
type FormValues = z.infer<typeof schema>

export default function InvoiceSheet({ invoiceId, onOpenChange, onChanged }: { invoiceId: string | null; onOpenChange: (open: boolean) => void; onChanged: () => void }) {
  const [detail, setDetail] = useState<InvoiceDetail | null>(null)
  const [loading, setLoading] = useState(false)
  const [deleting, setDeleting] = useState(false)
  const [deleteOpen, setDeleteOpen] = useState(false)
  const form = useForm<FormValues>({ resolver: zodResolver(schema), defaultValues: emptyValues })

  useEffect(() => {
    if (!invoiceId) { setDetail(null); return }
    setLoading(true)
    api.getInvoice(invoiceId).then((value) => {
      setDetail(value)
      form.reset(valuesFrom(value))
    }).catch((error) => toast.error(errorMessage(error))).finally(() => setLoading(false))
  }, [form, invoiceId])

  const save = form.handleSubmit(async (values) => {
    if (!invoiceId) return
    try {
      const saved = await api.updateInvoice({
        id: invoiceId,
        invoiceCode: optional(values.invoiceCode), invoiceNumber: optional(values.invoiceNumber), invoiceDate: optional(values.invoiceDate),
        buyerName: optional(values.buyerName), buyerTaxId: optional(values.buyerTaxId), sellerName: optional(values.sellerName), sellerTaxId: optional(values.sellerTaxId),
        amountFen: parseYuanToFen(values.amount), taxFen: parseYuanToFen(values.tax), totalFen: parseYuanToFen(values.total), currency: values.currency,
      })
      setDetail(saved)
      toast.success("发票已保存")
      onChanged()
    } catch (error) { toast.error(errorMessage(error)) }
  })

  const previewUrl = detail ? convertFileSrc(detail.previewPath) : ""
  const isPdf = detail?.previewPath.toLowerCase().endsWith(".pdf")

  return (
    <Sheet open={Boolean(invoiceId)} onOpenChange={onOpenChange}>
      <SheetContent className="w-full max-w-none gap-0 p-0" style={{ width: "min(980px, calc(100vw - 24px))", maxWidth: "none" }}>
        <SheetHeader className="border-b px-6 py-4">
          <div className="flex items-center gap-3"><SheetTitle>发票详情</SheetTitle>{detail?.possibleDuplicate && <Badge variant="destructive">可能重复</Badge>}{detail && <Badge variant="secondary">{detail.subtypeDescription}</Badge>}</div>
          <SheetDescription>{detail?.originalFileName ?? "读取发票信息"}</SheetDescription>
        </SheetHeader>
        {loading || !detail ? <div className="grid flex-1 grid-cols-2 gap-6 p-6"><Skeleton className="h-[560px]" /><div className="space-y-4">{Array.from({ length: 8 }, (_, i) => <Skeleton key={i} className="h-12" />)}</div></div> : (
          <div className="grid min-h-0 flex-1 grid-cols-[minmax(360px,1fr)_minmax(400px,1fr)] overflow-hidden">
            <ScrollArea className="border-r bg-zinc-100">
              {isPdf ? <PdfPreview url={previewUrl} label={detail.originalFileName} /> : <div className="p-4"><img src={previewUrl} alt={detail.originalFileName} className="h-auto w-full border bg-white object-contain" /></div>}
            </ScrollArea>
            <form onSubmit={save} className="flex min-h-0 flex-col">
              <ScrollArea className="flex-1"><div className="space-y-5 p-6">
                <div className="grid grid-cols-2 gap-4"><Field label="发票代码"><Input {...form.register("invoiceCode")} /></Field><Field label="发票号码"><Input {...form.register("invoiceNumber")} /></Field></div>
                <div className="grid grid-cols-2 gap-4"><Field label="开票日期"><Input placeholder="YYYY-MM-DD" {...form.register("invoiceDate")} /></Field><Field label="币种"><Input {...form.register("currency")} /></Field></div>
                <Party title="购买方" name="buyerName" tax="buyerTaxId" form={form} />
                <Party title="销售方" name="sellerName" tax="sellerTaxId" form={form} />
                <div className="grid grid-cols-3 gap-3"><Field label="不含税金额"><Input inputMode="decimal" {...form.register("amount")} /></Field><Field label="税额"><Input inputMode="decimal" {...form.register("tax")} /></Field><Field label="价税合计"><Input inputMode="decimal" {...form.register("total")} /></Field></div>
                {detail.items.length > 0 && <><Separator /><div><Label>发票明细</Label><div className="mt-2 divide-y border">{detail.items.map((item) => <div key={item.id} className="flex justify-between gap-4 px-3 py-2 text-sm"><span className="truncate">{item.name}</span><span className="shrink-0 tabular-nums">{item.amountFen === null ? "-" : `¥${(item.amountFen / 100).toFixed(2)}`}</span></div>)}</div></div></>}
                <Separator />
                <div className="space-y-2"><Label className="flex items-center gap-2"><FileJson2 className="size-4" />腾讯原始响应</Label><Textarea value={detail.rawResponseJson} readOnly className="min-h-40 resize-none bg-zinc-50 font-mono text-xs" /><p className="text-xs text-muted-foreground">RequestId: {detail.requestId ?? "-"}</p></div>
              </div></ScrollArea>
              <div className="flex items-center justify-between border-t px-6 py-4"><Button type="button" variant="ghost" className="text-destructive" onClick={() => setDeleteOpen(true)}><Trash2 />删除发票</Button><Button type="submit" disabled={form.formState.isSubmitting}>{form.formState.isSubmitting ? <LoaderCircle className="animate-spin" /> : <Save />}保存</Button></div>
            </form>
          </div>
        )}
      </SheetContent>
      <Dialog open={deleteOpen} onOpenChange={(open) => !deleting && setDeleteOpen(open)}><DialogContent><DialogHeader><DialogTitle>删除发票？</DialogTitle><DialogDescription>台账记录、拆分预览及最后一个引用的源文件将被删除。</DialogDescription></DialogHeader><DialogFooter><Button variant="outline" disabled={deleting} onClick={() => setDeleteOpen(false)}>取消</Button><Button variant="destructive" disabled={deleting} onClick={async () => { if (!detail) return; setDeleting(true); try { await api.deleteInvoice(detail.id); toast.success("发票已删除"); setDeleteOpen(false); onOpenChange(false); onChanged() } catch (error) { toast.error(errorMessage(error)) } finally { setDeleting(false) } }}>{deleting ? <LoaderCircle className="animate-spin" /> : <Trash2 />}确认删除</Button></DialogFooter></DialogContent></Dialog>
    </Sheet>
  )
}

const emptyValues: FormValues = { invoiceCode: "", invoiceNumber: "", invoiceDate: "", buyerName: "", buyerTaxId: "", sellerName: "", sellerTaxId: "", amount: "", tax: "", total: "", currency: "CNY" }
function valuesFrom(value: InvoiceDetail): FormValues { return { invoiceCode: value.invoiceCode ?? "", invoiceNumber: value.invoiceNumber ?? "", invoiceDate: value.invoiceDate ?? "", buyerName: value.buyerName ?? "", buyerTaxId: value.buyerTaxId ?? "", sellerName: value.sellerName ?? "", sellerTaxId: value.sellerTaxId ?? "", amount: amount(value.amountFen), tax: amount(value.taxFen), total: amount(value.totalFen), currency: value.currency } }
function amount(value: number | null) { return value === null ? "" : (value / 100).toFixed(2) }
function optional(value: string) { return value.trim() || null }
function Field({ label, children }: { label: string; children: React.ReactNode }) { return <div className="space-y-2"><Label>{label}</Label>{children}</div> }
function Party({ title, name, tax, form }: { title: string; name: "buyerName" | "sellerName"; tax: "buyerTaxId" | "sellerTaxId"; form: ReturnType<typeof useForm<FormValues>> }) { return <div className="space-y-3"><Label>{title}</Label><div className="grid grid-cols-2 gap-4"><Input aria-label={`${title}名称`} placeholder="名称" {...form.register(name)} /><Input aria-label={`${title}税号`} placeholder="纳税人识别号" {...form.register(tax)} /></div></div> }
