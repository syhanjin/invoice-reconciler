import { zodResolver } from "@hookform/resolvers/zod"
import { CheckCircle2, LoaderCircle, PlugZap, Save, Server, Terminal } from "lucide-react"
import { useEffect, useState } from "react"
import { useForm } from "react-hook-form"
import { toast } from "sonner"
import { z } from "zod"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
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
})

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
    defaultValues: settings,
  })

  useEffect(() => form.reset(settings), [form, settings])

  const save = form.handleSubmit(async (values) => {
    try {
      const saved = await api.saveSettings(values)
      onSettingsChange(saved)
      toast.success("设置已保存，将从下一个批次开始生效")
    } catch (error) {
      toast.error(errorMessage(error))
    }
  })

  const check = async () => {
    const valid = await form.trigger()
    if (!valid) return
    setChecking(true)
    setStatus(null)
    try {
      setStatus(await api.checkOllama(form.getValues()))
    } catch (error) {
      setStatus({
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
    <div className="min-h-screen px-8 py-7">
      <header className="mb-6">
        <h1 className="text-xl font-semibold">设置</h1>
        <p className="mt-1 text-sm text-muted-foreground">管理本地 OCR 服务与批处理资源。</p>
      </header>
      <div className="max-w-3xl space-y-5">
        <Card className="rounded-md shadow-none">
          <CardHeader>
            <div className="flex items-center gap-3">
              <div className="flex size-9 items-center justify-center rounded-md bg-zinc-100">
                <Server className="size-4" />
              </div>
              <div>
                <CardTitle className="text-base">Ollama</CardTitle>
                <CardDescription>连接地址和视觉模型</CardDescription>
              </div>
            </div>
          </CardHeader>
          <CardContent>
            <form onSubmit={save} className="space-y-5">
              <div className="grid grid-cols-[1fr_220px] gap-4">
                <div className="space-y-2">
                  <Label htmlFor="ollama-url">服务地址</Label>
                  <Input id="ollama-url" {...form.register("ollamaBaseUrl")} />
                  {form.formState.errors.ollamaBaseUrl && (
                    <p className="text-xs text-destructive">
                      {form.formState.errors.ollamaBaseUrl.message}
                    </p>
                  )}
                </div>
                <div className="space-y-2">
                  <Label htmlFor="ollama-model">视觉模型</Label>
                  <Input id="ollama-model" {...form.register("ollamaModel")} />
                </div>
              </div>
              <div className="space-y-2">
                <Label htmlFor="concurrency">最大同时请求数</Label>
                <div className="flex items-center gap-3">
                  <Input
                    id="concurrency"
                    type="number"
                    min={1}
                    max={8}
                    className="w-28"
                    {...form.register("maxConcurrency", { valueAsNumber: true })}
                  />
                  <span className="text-xs text-muted-foreground">
                    取值 1–8，当前批次运行中修改不会改变该批次并发。
                  </span>
                </div>
              </div>
              <div className="flex items-center gap-2 pt-1">
                <Button type="submit" disabled={form.formState.isSubmitting}>
                  {form.formState.isSubmitting ? <LoaderCircle className="animate-spin" /> : <Save />}
                  保存设置
                </Button>
                <Button type="button" variant="outline" disabled={checking} onClick={check}>
                  {checking ? <LoaderCircle className="animate-spin" /> : <PlugZap />}
                  测试连接
                </Button>
              </div>
            </form>
          </CardContent>
        </Card>

        {status && (
          <Alert
            className={
              status.reachable && status.modelAvailable
                ? "rounded-md border-emerald-200 bg-emerald-50"
                : "rounded-md border-amber-200 bg-amber-50"
            }
          >
            {status.reachable && status.modelAvailable ? <CheckCircle2 /> : <Terminal />}
            <AlertTitle>
              {status.reachable && status.modelAvailable ? "连接正常" : "需要处理"}
            </AlertTitle>
            <AlertDescription>
              <p>{status.message}</p>
              {!status.reachable && (
                <code className="mt-2 block rounded bg-white/70 px-3 py-2 text-xs">ollama serve</code>
              )}
              {status.reachable && !status.modelAvailable && (
                <code className="mt-2 block rounded bg-white/70 px-3 py-2 text-xs">
                  ollama pull {form.getValues("ollamaModel")}
                </code>
              )}
            </AlertDescription>
          </Alert>
        )}
      </div>
    </div>
  )
}
