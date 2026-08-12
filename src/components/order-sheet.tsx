import { zodResolver } from "@hookform/resolvers/zod"
import { convertFileSrc } from "@tauri-apps/api/core"
import { AlertTriangle, LoaderCircle, Plus, Save, Trash2, X } from "lucide-react"
import { useEffect, useState } from "react"
import { useFieldArray, useForm } from "react-hook-form"
import { toast } from "sonner"
import { z } from "zod"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Separator } from "@/components/ui/separator"
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
} from "@/components/ui/sheet"
import { Skeleton } from "@/components/ui/skeleton"
import { Textarea } from "@/components/ui/textarea"
import { api, errorMessage } from "@/lib/api"
import { parseYuanToFen } from "@/lib/format"
import type { OrderDetail } from "@/lib/types"

const formSchema = z.object({
  externalOrderNumber: z.string().max(100),
  products: z.array(z.object({ name: z.string().max(500) })).max(100),
  paidAmount: z
    .string()
    .refine((value) => value === "" || /^\d+(?:\.\d{1,2})?$/.test(value), "请输入有效金额"),
  orderedAt: z.string().max(32),
  shopName: z.string().max(200),
})

type OrderForm = z.infer<typeof formSchema>

interface OrderSheetProps {
  orderId: string | null
  onOpenChange: (open: boolean) => void
  onChanged: () => void
}

export default function OrderSheet({ orderId, onOpenChange, onChanged }: OrderSheetProps) {
  const [detail, setDetail] = useState<OrderDetail | null>(null)
  const [loading, setLoading] = useState(false)
  const [pendingSave, setPendingSave] = useState<OrderForm | null>(null)
  const form = useForm<OrderForm>({
    resolver: zodResolver(formSchema),
    defaultValues: {
      externalOrderNumber: "",
      products: [],
      paidAmount: "",
      orderedAt: "",
      shopName: "",
    },
  })
  const products = useFieldArray({ control: form.control, name: "products" })

  useEffect(() => {
    setPendingSave(null)
    if (!orderId) {
      setDetail(null)
      return
    }
    setLoading(true)
    api
      .getOrder(orderId)
      .then((order) => {
        setDetail(order)
        form.reset({
          externalOrderNumber: order.externalOrderNumber ?? "",
          products: order.productNames.map((name) => ({ name })),
          paidAmount: order.paidAmountFen === null ? "" : (order.paidAmountFen / 100).toFixed(2),
          orderedAt: order.orderedAt ?? "",
          shopName: order.shopName ?? "",
        })
      })
      .catch((error) => toast.error(errorMessage(error)))
      .finally(() => setLoading(false))
  }, [form, orderId])

  const persist = async (values: OrderForm) => {
    if (!orderId) return
    const paidAmountFen = parseYuanToFen(values.paidAmount)
    if (Number.isNaN(paidAmountFen)) {
      form.setError("paidAmount", { message: "请输入有效金额" })
      return
    }
    try {
      const saved = await api.updateOrder({
        id: orderId,
        externalOrderNumber: values.externalOrderNumber.trim() || null,
        productNames: values.products.map((item) => item.name.trim()).filter(Boolean),
        paidAmountFen,
        orderedAt: values.orderedAt.trim() || null,
        shopName: values.shopName.trim() || null,
      })
      setDetail(saved)
      toast.success("订单已保存")
      onChanged()
    } catch (error) {
      toast.error(errorMessage(error))
    }
  }

  const save = form.handleSubmit(async (values) => {
    const paidAmountFen = parseYuanToFen(values.paidAmount)
    const changesMatchingFields = detail?.isMatched && (
      paidAmountFen !== detail.paidAmountFen
      || (values.orderedAt.trim() || null) !== detail.orderedAt
      || (values.shopName.trim() || null) !== detail.shopName
    )
    if (changesMatchingFields) {
      setPendingSave(values)
      return
    }
    await persist(values)
  })

  return (
    <Sheet open={Boolean(orderId)} onOpenChange={onOpenChange}>
      <SheetContent
        className="w-full max-w-none gap-0 p-0"
        style={{ width: "min(800px, calc(100vw - 24px))", maxWidth: "none" }}
      >
        <SheetHeader className="border-b px-6 py-4">
          <div className="flex items-center gap-3">
            <SheetTitle>订单详情</SheetTitle>
            {detail?.possibleDuplicate && <Badge variant="destructive">可能重复</Badge>}
            {detail?.isMatched && <Badge variant="outline">已匹配</Badge>}
          </div>
          <SheetDescription>{detail?.originalFileName ?? "读取订单信息"}</SheetDescription>
        </SheetHeader>
        {loading || !detail ? (
          <div className="grid flex-1 grid-cols-2 gap-6 p-6">
            <Skeleton className="h-[540px] rounded-md" />
            <div className="space-y-4">
              {Array.from({ length: 6 }, (_, index) => (
                <Skeleton key={index} className="h-12" />
              ))}
            </div>
          </div>
        ) : (
          <div className="grid min-h-0 flex-1 grid-cols-1 overflow-y-auto min-[760px]:grid-cols-[minmax(280px,0.9fr)_minmax(340px,1.1fr)] min-[760px]:overflow-hidden">
            <div className="flex min-h-0 flex-col border-r bg-zinc-50">
              <div className="border-b px-4 py-3 text-xs font-medium text-muted-foreground">
                原始截图
              </div>
              <ScrollArea className="flex-1">
                <div className="p-4">
                  <img
                    src={convertFileSrc(detail.imagePath)}
                    alt={detail.originalFileName}
                    className="h-auto w-full rounded-md border bg-white object-contain"
                  />
                </div>
              </ScrollArea>
            </div>
            <form onSubmit={save} className="flex min-h-0 flex-col">
              <ScrollArea className="flex-1">
                <div className="space-y-5 p-6">
                  {detail.warningMessage && (
                    <Alert className="rounded-md border-amber-200 bg-amber-50">
                      <AlertTriangle />
                      <AlertTitle>识别结果需要确认</AlertTitle>
                      <AlertDescription>{detail.warningMessage}</AlertDescription>
                    </Alert>
                  )}
                  <div className="space-y-2">
                    <Label htmlFor="order-number">订单号</Label>
                    <Input id="order-number" {...form.register("externalOrderNumber")} />
                  </div>
                  <div className="space-y-2">
                    <div className="flex items-center justify-between">
                      <Label>商品</Label>
                      <Button
                        type="button"
                        variant="ghost"
                        size="sm"
                        onClick={() => products.append({ name: "" })}
                      >
                        <Plus />
                        添加商品
                      </Button>
                    </div>
                    <div className="space-y-2">
                      {products.fields.length === 0 && (
                        <p className="rounded-md border border-dashed p-3 text-xs text-muted-foreground">
                          暂无商品
                        </p>
                      )}
                      {products.fields.map((field, index) => (
                        <div key={field.id} className="flex gap-2">
                          <Input
                            aria-label={`商品 ${index + 1}`}
                            {...form.register(`products.${index}.name`)}
                          />
                          <Button
                            type="button"
                            variant="ghost"
                            size="icon"
                            onClick={() => products.remove(index)}
                            title="移除商品"
                          >
                            <X />
                          </Button>
                        </div>
                      ))}
                    </div>
                  </div>
                  <div className="grid grid-cols-2 gap-4">
                    <div className="space-y-2">
                      <Label htmlFor="paid-amount">实付金额（元）</Label>
                      <Input id="paid-amount" inputMode="decimal" {...form.register("paidAmount")} />
                      {form.formState.errors.paidAmount && (
                        <p className="text-xs text-destructive">
                          {form.formState.errors.paidAmount.message}
                        </p>
                      )}
                    </div>
                    <div className="space-y-2">
                      <Label htmlFor="ordered-at">下单时间</Label>
                      <Input
                        id="ordered-at"
                        placeholder="YYYY-MM-DD HH:mm"
                        {...form.register("orderedAt")}
                      />
                    </div>
                  </div>
                  <div className="space-y-2">
                    <Label htmlFor="shop-name">店铺名称</Label>
                    <Input id="shop-name" {...form.register("shopName")} />
                  </div>
                  <Separator />
                  <div className="space-y-2">
                    <Label htmlFor="raw-json">原始 OCR JSON</Label>
                    <Textarea
                      id="raw-json"
                      value={detail.rawOcrJson}
                      readOnly
                      className="min-h-32 resize-none bg-zinc-50 font-mono text-xs"
                    />
                  </div>
                </div>
              </ScrollArea>
              <div className="flex items-center justify-between border-t px-6 py-4">
                <Dialog>
                  <DialogTrigger
                    render={<Button type="button" variant="ghost" className="text-destructive" />}
                  >
                    <Trash2 />
                    删除订单
                  </DialogTrigger>
                  <DialogContent>
                    <DialogHeader>
                      <DialogTitle>删除订单？</DialogTitle>
                      <DialogDescription>
                        {detail.isMatched ? "该订单已确认匹配，请先在匹配页撤销确认。" : "订单记录和受管截图将一起删除，此操作无法撤销。"}
                      </DialogDescription>
                    </DialogHeader>
                    <DialogFooter>
                      <DialogClose render={<Button variant="outline" />}>
                        取消
                      </DialogClose>
                      <Button
                        variant="destructive"
                        disabled={detail.isMatched}
                        onClick={async () => {
                          try {
                            await api.deleteOrder(detail.id)
                            toast.success("订单已删除")
                            onChanged()
                            onOpenChange(false)
                          } catch (error) {
                            toast.error(errorMessage(error))
                          }
                        }}
                      >
                        <Trash2 />
                        确认删除
                      </Button>
                    </DialogFooter>
                  </DialogContent>
                </Dialog>
                <Button type="submit" disabled={form.formState.isSubmitting}>
                  {form.formState.isSubmitting ? (
                    <LoaderCircle className="animate-spin" />
                  ) : (
                    <Save />
                  )}
                  保存
                </Button>
              </div>
            </form>
          </div>
        )}
      </SheetContent>
      <Dialog open={Boolean(pendingSave)} onOpenChange={(open) => !open && setPendingSave(null)}>
        <DialogContent>
          <DialogHeader><DialogTitle>保存并解除匹配？</DialogTitle><DialogDescription>实付款、下单时间或店铺名称发生变化。继续保存会解除该订单当前已确认的发票匹配。</DialogDescription></DialogHeader>
          <DialogFooter><Button variant="outline" onClick={() => setPendingSave(null)}>取消</Button><Button onClick={async () => { const values = pendingSave; setPendingSave(null); if (values) await persist(values) }}>继续保存</Button></DialogFooter>
        </DialogContent>
      </Dialog>
    </Sheet>
  )
}
