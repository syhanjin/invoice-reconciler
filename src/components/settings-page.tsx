import { zodResolver } from "@hookform/resolvers/zod"
import {
  AlertTriangle,
  CheckCircle2,
  CloudCog,
  FileKey2,
  GitCompareArrows,
  MessageSquareText,
  LoaderCircle,
  PlugZap,
  Save,
  ScanLine,
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
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { api, errorMessage } from "@/lib/api"
import type { AppSettings, LlmProvider, LlmStatus, SaveSettingsRequest } from "@/lib/types"

const httpUrl = z
  .url("请输入有效地址")
  .refine((value) => value.startsWith("http://") || value.startsWith("https://"), "仅支持 HTTP(S) 地址")

const settingsSchema = z.object({
  llmProvider: z.enum(["ollama", "openai"]),
  ollamaBaseUrl: httpUrl,
  ollamaModel: z.string().trim().min(1, "模型名称不能为空").max(200),
  openaiBaseUrl: httpUrl,
  openaiModel: z.string().trim().min(1, "模型名称不能为空").max(200),
  openaiApiKey: z.string().trim().max(500),
  clearOpenaiApiKey: z.boolean(),
  orderRecognitionMaxConcurrency: z.number().int().min(1).max(8),
  tencentSecretId: z.string().trim().max(200),
  tencentSecretKey: z.string().trim().max(200),
  clearTencentCredentials: z.boolean(),
  invoiceOcrConcurrency: z.number().int().min(1).max(5),
  matchMaxOrders: z.number().int().min(2).max(20),
  matchMaxCandidates: z.number().int().min(10).max(500),
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

const providerNames: Record<LlmProvider, string> = {
  ollama: "Ollama",
  openai: "OpenAI 兼容",
}

export default function SettingsPage({ settings, onSettingsChange }: SettingsPageProps) {
  const [status, setStatus] = useState<LlmStatus | null>(null)
  const [checking, setChecking] = useState(false)
  const form = useForm<SettingsForm>({
    resolver: zodResolver(settingsSchema),
    defaultValues: formValues(settings),
  })
  const provider = form.watch("llmProvider")
  const clearOpenaiApiKey = form.watch("clearOpenaiApiKey")
  const clearTencentCredentials = form.watch("clearTencentCredentials")

  useEffect(() => form.reset(formValues(settings)), [form, settings])

  const save = form.handleSubmit(async (values) => {
    try {
      const saved = await api.saveSettings(requestFrom(values))
      onSettingsChange(saved)
      form.reset(formValues(saved))
      setStatus(null)
      toast.success("设置已保存")
    } catch (error) {
      toast.error(errorMessage(error))
    }
  })

  const check = async () => {
    const fields = provider === "ollama"
      ? (["ollamaBaseUrl", "ollamaModel"] as const)
      : (["openaiBaseUrl", "openaiModel"] as const)
    if (!await form.trigger(fields)) return
    setChecking(true)
    setStatus(null)
    try {
      setStatus(await api.checkLlm(requestFrom(form.getValues())))
    } catch (error) {
      setStatus({
        provider,
        reachable: false,
        modelAvailable: false,
        version: null,
        message: errorMessage(error),
      })
    } finally {
      setChecking(false)
    }
  }

  return (
    <div className="h-screen overflow-y-auto px-5 py-6 md:px-8 md:py-7">
      <header className="mb-6">
        <h1 className="text-xl font-semibold">设置</h1>
        <p className="mt-1 text-sm text-muted-foreground">管理模型、订单识别、发票识别与自动匹配参数。</p>
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
              <div className="flex size-9 items-center justify-center rounded-md bg-zinc-100"><MessageSquareText className="size-4" /></div>
              <div><CardTitle className="text-base">模型配置</CardTitle><CardDescription>用于订单图片识别和确认组合商品摘要</CardDescription></div>
            </div>
          </CardHeader>
          <CardContent className="space-y-5">
            <Field label="模型服务">
              <Select
                value={provider}
                onValueChange={(value) => {
                  form.setValue("llmProvider", value as LlmProvider, { shouldDirty: true })
                  setStatus(null)
                }}
              >
                <SelectTrigger className="w-full sm:w-64"><SelectValue /></SelectTrigger>
                <SelectContent>
                  <SelectItem value="ollama">Ollama</SelectItem>
                  <SelectItem value="openai">OpenAI 兼容</SelectItem>
                </SelectContent>
              </Select>
            </Field>

            {provider === "ollama" && (
              <div className="grid gap-4 md:grid-cols-[minmax(0,1fr)_220px]">
                <Field label="服务地址" error={form.formState.errors.ollamaBaseUrl?.message}>
                  <Input {...form.register("ollamaBaseUrl")} />
                </Field>
                <Field label="模型" error={form.formState.errors.ollamaModel?.message}>
                  <Input {...form.register("ollamaModel")} />
                </Field>
              </div>
            )}

            {provider === "openai" && (
              <>
                <div className="grid gap-4 md:grid-cols-[minmax(0,1fr)_220px]">
                  <Field label="API 地址" error={form.formState.errors.openaiBaseUrl?.message}>
                    <Input {...form.register("openaiBaseUrl")} />
                  </Field>
                  <Field label="模型" error={form.formState.errors.openaiModel?.message}>
                    <Input {...form.register("openaiModel")} />
                  </Field>
                </div>
                <CredentialField
                  label="API Key"
                  configured={settings.openaiApiKeyConfigured}
                  masked={settings.openaiApiKeyMasked}
                  clear={clearOpenaiApiKey}
                  onClearChange={(checked) => form.setValue("clearOpenaiApiKey", checked, { shouldDirty: true })}
                  inputProps={form.register("openaiApiKey")}
                />
              </>
            )}

            <Button type="button" variant="outline" disabled={checking} onClick={check}>
              {checking ? <LoaderCircle className="animate-spin" /> : <PlugZap />}测试连接
            </Button>
          </CardContent>
        </Card>

        {status && (
          <Alert className={status.reachable && status.modelAvailable ? "rounded-md border-emerald-200 bg-emerald-50" : "rounded-md border-amber-200 bg-amber-50"}>
            {status.reachable && status.modelAvailable ? <CheckCircle2 /> : <Terminal />}
            <AlertTitle>{status.reachable && status.modelAvailable ? `${providerNames[status.provider]} 连接正常` : `${providerNames[status.provider]} 需要处理`}</AlertTitle>
            <AlertDescription>{status.message}</AlertDescription>
          </Alert>
        )}

        <Card className="rounded-md shadow-none">
          <CardHeader>
            <div className="flex items-center gap-3">
              <div className="flex size-9 items-center justify-center rounded-md bg-zinc-100"><ScanLine className="size-4" /></div>
              <div><CardTitle className="text-base">订单识别</CardTitle><CardDescription>实时处理订单图片任务池</CardDescription></div>
            </div>
          </CardHeader>
          <CardContent>
            <Field label="最大同时请求数" error={form.formState.errors.orderRecognitionMaxConcurrency?.message}>
              <Input type="number" min={1} max={8} className="w-28" {...form.register("orderRecognitionMaxConcurrency", { valueAsNumber: true })} />
            </Field>
          </CardContent>
        </Card>

        <Card className="rounded-md shadow-none">
          <CardHeader>
            <div className="flex items-center gap-3">
              <div className="flex size-9 items-center justify-center rounded-md bg-zinc-100"><GitCompareArrows className="size-4" /></div>
              <div><CardTitle className="text-base">自动匹配</CardTitle><CardDescription>限制多订单候选组合的搜索规模</CardDescription></div>
            </div>
          </CardHeader>
          <CardContent className="grid gap-4 sm:grid-cols-2">
            <Field label="单个组合最多订单数" error={form.formState.errors.matchMaxOrders?.message}>
              <Input type="number" min={2} max={20} className="w-32" {...form.register("matchMaxOrders", { valueAsNumber: true })} />
            </Field>
            <Field label="每张发票最多候选数" error={form.formState.errors.matchMaxCandidates?.message}>
              <Input type="number" min={10} max={500} className="w-32" {...form.register("matchMaxCandidates", { valueAsNumber: true })} />
            </Field>
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
            <div className="grid gap-4 sm:grid-cols-2">
              <Field label="SecretId">
                <Input aria-label="SecretId" autoComplete="off" disabled={clearTencentCredentials} placeholder={settings.tencentCredentialsConfigured ? "留空保持现有值" : "AKID..."} {...form.register("tencentSecretId")} />
              </Field>
              <Field label="SecretKey" error={form.formState.errors.tencentSecretKey?.message}>
                <Input aria-label="SecretKey" type="password" autoComplete="new-password" disabled={clearTencentCredentials} placeholder={settings.tencentCredentialsConfigured ? "留空保持现有值" : "输入 SecretKey"} {...form.register("tencentSecretKey")} />
              </Field>
            </div>
            <div className="flex flex-wrap items-center gap-3">
              <Field label="OCR 最大同时请求数">
                <Input type="number" min={1} max={5} className="w-28" {...form.register("invoiceOcrConcurrency", { valueAsNumber: true })} />
              </Field>
              <p className="mt-6 text-xs text-muted-foreground">取值 1-5，全局请求频率上限为 5 次/秒。</p>
            </div>
            {settings.tencentCredentialsConfigured && (
              <label className="flex w-fit cursor-pointer items-center gap-2 text-sm">
                <Checkbox checked={clearTencentCredentials} onCheckedChange={(checked) => form.setValue("clearTencentCredentials", checked === true, { shouldDirty: true })} />
                清除已保存的腾讯云凭据
              </label>
            )}
          </CardContent>
        </Card>

        <div className="sticky bottom-0 flex justify-end border-t bg-background/95 py-4 backdrop-blur">
          <Button type="submit" disabled={form.formState.isSubmitting}>
            {form.formState.isSubmitting ? <LoaderCircle className="animate-spin" /> : <Save />}保存全部设置
          </Button>
        </div>
      </form>
    </div>
  )
}

function requestFrom(values: SettingsForm): SaveSettingsRequest {
  return {
    llmProvider: values.llmProvider,
    ollamaBaseUrl: values.ollamaBaseUrl,
    ollamaModel: values.ollamaModel,
    openaiBaseUrl: values.openaiBaseUrl,
    openaiModel: values.openaiModel,
    openaiApiKey: values.openaiApiKey || null,
    clearOpenaiApiKey: values.clearOpenaiApiKey,
    orderRecognitionMaxConcurrency: values.orderRecognitionMaxConcurrency,
    tencentSecretId: values.tencentSecretId || null,
    tencentSecretKey: values.tencentSecretKey || null,
    clearTencentCredentials: values.clearTencentCredentials,
    invoiceOcrConcurrency: values.invoiceOcrConcurrency,
    matchMaxOrders: values.matchMaxOrders,
    matchMaxCandidates: values.matchMaxCandidates,
  }
}

function formValues(settings: AppSettings): SettingsForm {
  return {
    llmProvider: settings.llmProvider,
    ollamaBaseUrl: settings.ollamaBaseUrl,
    ollamaModel: settings.ollamaModel,
    openaiBaseUrl: settings.openaiBaseUrl,
    openaiModel: settings.openaiModel,
    openaiApiKey: "",
    clearOpenaiApiKey: false,
    orderRecognitionMaxConcurrency: settings.orderRecognitionMaxConcurrency,
    tencentSecretId: "",
    tencentSecretKey: "",
    clearTencentCredentials: false,
    invoiceOcrConcurrency: settings.invoiceOcrConcurrency,
    matchMaxOrders: settings.matchMaxOrders,
    matchMaxCandidates: settings.matchMaxCandidates,
  }
}

function CredentialField({ label, configured, masked, clear, onClearChange, inputProps }: {
  label: string
  configured: boolean
  masked: string | null
  clear: boolean
  onClearChange: (checked: boolean) => void
  inputProps: React.ComponentProps<typeof Input>
}) {
  return (
    <div className="space-y-3">
      <Field label={label}>
        <Input
          type="password"
          autoComplete="new-password"
          disabled={clear}
          placeholder={configured ? `已配置 ${masked ?? "访问密钥"}，留空保持` : "输入访问密钥"}
          {...inputProps}
        />
      </Field>
      {configured && (
        <label className="flex w-fit cursor-pointer items-center gap-2 text-sm">
          <Checkbox checked={clear} onCheckedChange={(checked) => onClearChange(checked === true)} />
          清除已保存的 {label}
        </label>
      )}
    </div>
  )
}

function Field({ label, error, children }: { label: string; error?: string; children: React.ReactNode }) {
  return <div className="space-y-2"><Label>{label}</Label>{children}{error && <p className="text-xs text-destructive">{error}</p>}</div>
}
