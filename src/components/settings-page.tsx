import { zodResolver } from "@hookform/resolvers/zod"
import {
  AlertTriangle,
  CheckCircle2,
  CloudCog,
  FileKey2,
  LoaderCircle,
  PlugZap,
  Save,
  Server,
  Terminal,
} from "lucide-react"
import { useEffect, useState } from "react"
import { useForm } from "react-hook-form"
import { toast } from "sonner"
import { z } from "zod"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Checkbox } from "@/components/ui/checkbox"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { api, errorMessage } from "@/lib/api"
import type { AppSettings, OllamaStatus } from "@/lib/types"

const settingsSchema = z.object({
  ollamaBaseUrl: z
    .url("请输入有效地址")
    .refine((value) => value.startsWith("http://") || value.startsWith("https://"), "仅支持 HTTP(S) 地址"),
  ollamaModel: z.string().trim().min(1, "模型名称不能为空").max(200),
  maxConcurrency: z.number().int().min(1).max(8),
  tencentSecretId: z.string().trim().max(200),
  tencentSecretKey: z.string().trim().max(200),
  clearTencentCredentials: z.boolean(),
  invoiceOcrConcurrency: z.number().int().min(1).max(5),
}).refine(
  (value) => value.clearTencentCredentials
    || (!value.tencentSecretId && !value.tencentSecretKey)
    || (Boolean(value.tencentSecretId) && Boolean(value.tencentSecretKey)),
  { message: "SecretId 和 SecretKey 必须同时填写", path: ["tencentSecretKey"] },
)

type SettingsForm = z.infer<typeof settingsSchema>

interface SettingsPageProps {
  settings: AppSettings
  onSettingsChange: (settings: AppSettings) => void
}

export default function SettingsPage({ settings, onSettingsChange }: SettingsPageProps) {
  const [status, setStatus] = useState<OllamaStatus | null>(null)
  const [checking, setChecking] = useState(false)
  const form = useForm<SettingsForm>({
    resolver: zodResolver(settingsSchema),
    defaultValues: formValues(settings),
  })
  const clearCredentials = form.watch("clearTencentCredentials")

  useEffect(() => form.reset(formValues(settings)), [form, settings])

  const requestFrom = (values: SettingsForm) => ({
    ollamaBaseUrl: values.ollamaBaseUrl,
    ollamaModel: values.ollamaModel,
    maxConcurrency: values.maxConcurrency,
    tencentSecretId: values.tencentSecretId || null,
    tencentSecretKey: values.tencentSecretKey || null,
    clearTencentCredentials: values.clearTencentCredentials,
    invoiceOcrConcurrency: values.invoiceOcrConcurrency,
  })

  const save = form.handleSubmit(async (values) => {
    try {
      const saved = await api.saveSettings(requestFrom(values))
      onSettingsChange(saved)
      form.reset(formValues(saved))
      toast.success("设置已保存，将从下一个批次开始生效")
    } catch (error) {
      toast.error(errorMessage(error))
    }
  })

  const check = async () => {
    const valid = await form.trigger(["ollamaBaseUrl", "ollamaModel", "maxConcurrency"])
    if (!valid) return
    setChecking(true)
    setStatus(null)
    try {
      setStatus(await api.checkOllama(requestFrom(form.getValues())))
    } catch (error) {
      setStatus({ reachable: false, modelAvailable: false, version: null, message: errorMessage(error) })
    } finally {
      setChecking(false)
    }
  }

  return (
    <div className="h-screen overflow-y-auto px-8 py-7">
      <header className="mb-6">
        <h1 className="text-xl font-semibold">设置</h1>
        <p className="mt-1 text-sm text-muted-foreground">管理本地识别服务、腾讯云凭据与任务并发。</p>
      </header>
      <form onSubmit={save} className="max-w-3xl space-y-5">
        {settings.permissionWarning && (
          <Alert className="rounded-md border-red-200 bg-red-50 text-red-900">
            <AlertTriangle />
            <AlertTitle>设置文件权限未能收紧</AlertTitle>
            <AlertDescription>{settings.permissionWarning}</AlertDescription>
          </Alert>
        )}

        <Card className="rounded-md shadow-none">
          <CardHeader>
            <div className="flex items-center gap-3">
              <div className="flex size-9 items-center justify-center rounded-md bg-zinc-100"><Server className="size-4" /></div>
              <div><CardTitle className="text-base">Ollama</CardTitle><CardDescription>订单截图使用的本地视觉模型</CardDescription></div>
            </div>
          </CardHeader>
          <CardContent className="space-y-5">
            <div className="grid grid-cols-[1fr_220px] gap-4">
              <Field label="服务地址" error={form.formState.errors.ollamaBaseUrl?.message}>
                <Input {...form.register("ollamaBaseUrl")} />
              </Field>
              <Field label="视觉模型" error={form.formState.errors.ollamaModel?.message}>
                <Input {...form.register("ollamaModel")} />
              </Field>
            </div>
            <Field label="最大同时请求数">
              <Input type="number" min={1} max={8} className="w-28" {...form.register("maxConcurrency", { valueAsNumber: true })} />
            </Field>
            <Button type="button" variant="outline" disabled={checking} onClick={check}>
              {checking ? <LoaderCircle className="animate-spin" /> : <PlugZap />}测试 Ollama
            </Button>
          </CardContent>
        </Card>

        <Card className="rounded-md shadow-none">
          <CardHeader>
            <div className="flex items-center gap-3">
              <div className="flex size-9 items-center justify-center rounded-md bg-zinc-100"><CloudCog className="size-4" /></div>
              <div>
                <CardTitle className="text-base">腾讯云发票识别</CardTitle>
                <CardDescription>{settings.tencentCredentialsConfigured ? `已配置 ${settings.tencentSecretIdMasked}` : "尚未配置访问密钥"}</CardDescription>
              </div>
            </div>
          </CardHeader>
          <CardContent className="space-y-5">
            <Alert className="rounded-md border-amber-200 bg-amber-50">
              <FileKey2 />
              <AlertTitle>凭据保存在本机设置文件</AlertTitle>
              <AlertDescription className="break-all">{settings.settingsPath}</AlertDescription>
            </Alert>
            <div className="grid grid-cols-2 gap-4">
              <Field label="SecretId">
                <Input aria-label="SecretId" autoComplete="off" disabled={clearCredentials} placeholder={settings.tencentCredentialsConfigured ? "留空保持现有值" : "AKID..."} {...form.register("tencentSecretId")} />
              </Field>
              <Field label="SecretKey" error={form.formState.errors.tencentSecretKey?.message}>
                <Input aria-label="SecretKey" type="password" autoComplete="new-password" disabled={clearCredentials} placeholder={settings.tencentCredentialsConfigured ? "留空保持现有值" : "输入 SecretKey"} {...form.register("tencentSecretKey")} />
              </Field>
            </div>
            <div className="flex items-center gap-3">
              <Field label="OCR 最大同时请求数">
                <Input type="number" min={1} max={5} className="w-28" {...form.register("invoiceOcrConcurrency", { valueAsNumber: true })} />
              </Field>
              <p className="mt-6 text-xs text-muted-foreground">取值 1-5，系统同时限制全局请求频率不超过 5 次/秒。</p>
            </div>
            {settings.tencentCredentialsConfigured && (
              <label className="flex w-fit cursor-pointer items-center gap-2 text-sm">
                <Checkbox checked={clearCredentials} onCheckedChange={(checked) => form.setValue("clearTencentCredentials", checked === true)} />
                清除已保存的腾讯云凭据
              </label>
            )}
          </CardContent>
        </Card>

        {status && (
          <Alert className={status.reachable && status.modelAvailable ? "rounded-md border-emerald-200 bg-emerald-50" : "rounded-md border-amber-200 bg-amber-50"}>
            {status.reachable && status.modelAvailable ? <CheckCircle2 /> : <Terminal />}
            <AlertTitle>{status.reachable && status.modelAvailable ? "Ollama 连接正常" : "Ollama 需要处理"}</AlertTitle>
            <AlertDescription>{status.message}</AlertDescription>
          </Alert>
        )}

        <div className="sticky bottom-0 flex justify-end border-t bg-background/95 py-4 backdrop-blur">
          <Button type="submit" disabled={form.formState.isSubmitting}>
            {form.formState.isSubmitting ? <LoaderCircle className="animate-spin" /> : <Save />}保存全部设置
          </Button>
        </div>
      </form>
    </div>
  )
}

function formValues(settings: AppSettings): SettingsForm {
  return {
    ollamaBaseUrl: settings.ollamaBaseUrl,
    ollamaModel: settings.ollamaModel,
    maxConcurrency: settings.maxConcurrency,
    tencentSecretId: "",
    tencentSecretKey: "",
    clearTencentCredentials: false,
    invoiceOcrConcurrency: settings.invoiceOcrConcurrency,
  }
}

function Field({ label, error, children }: { label: string; error?: string; children: React.ReactNode }) {
  return <div className="space-y-2"><Label>{label}</Label>{children}{error && <p className="text-xs text-destructive">{error}</p>}</div>
}
