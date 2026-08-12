import {
  AlertCircle,
  Ban,
  CheckCircle2,
  ChevronDown,
  ChevronUp,
  Clock3,
  LoaderCircle,
  RefreshCw,
  RotateCcw,
  SkipForward,
  XCircle,
} from "lucide-react"
import { useState } from "react"

import { Alert, AlertDescription } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Progress } from "@/components/ui/progress"
import { ScrollArea } from "@/components/ui/scroll-area"
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip"
import { api, errorMessage } from "@/lib/api"
import { formatDateTime } from "@/lib/format"
import type { BatchDetail, ImportTask, TaskStatus } from "@/lib/types"
import { cn } from "@/lib/utils"
import { toast } from "sonner"

interface BatchPanelProps {
  batch: BatchDetail
  onBatchChange: (batch: BatchDetail) => void
}

const statusMeta: Record<
  TaskStatus,
  { label: string; icon: typeof Clock3; className: string }
> = {
  queued: { label: "排队", icon: Clock3, className: "text-zinc-600" },
  processing: { label: "识别中", icon: LoaderCircle, className: "text-blue-600" },
  succeeded: { label: "已入库", icon: CheckCircle2, className: "text-emerald-600" },
  failed: { label: "失败", icon: XCircle, className: "text-red-600" },
  canceled: { label: "已取消", icon: Ban, className: "text-zinc-500" },
  skipped: { label: "已跳过", icon: SkipForward, className: "text-amber-600" },
  interrupted: { label: "已中断", icon: AlertCircle, className: "text-orange-600" },
}

function TaskRow({ task, onRetry, onCancel }: {
  task: ImportTask
  onRetry: (id: string) => void
  onCancel: (id: string) => void
}) {
  const meta = statusMeta[task.status]
  const Icon = meta.icon
  const canRetry = task.status === "failed" || task.status === "interrupted"
  return (
    <div className="grid min-h-12 grid-cols-[minmax(0,1fr)_110px_80px] items-center gap-3 border-b px-4 py-2 last:border-0">
      <div className="min-w-0">
        <p className="truncate text-sm font-medium">{task.originalFileName}</p>
        {(task.errorMessage || task.warningMessage) && (
          <p className="mt-0.5 truncate text-xs text-muted-foreground">
            {task.errorMessage || task.warningMessage}
          </p>
        )}
      </div>
      <div className={cn("flex items-center gap-2 text-xs", meta.className)}>
        <Icon className={cn("size-3.5", task.status === "processing" && "animate-spin")} />
        {meta.label}
      </div>
      <div className="flex justify-end">
        {canRetry && (
          <Tooltip>
            <TooltipTrigger
              render={<Button variant="ghost" size="icon-sm" onClick={() => onRetry(task.id)} />}
            >
              <RotateCcw />
            </TooltipTrigger>
            <TooltipContent>重试该图片</TooltipContent>
          </Tooltip>
        )}
        {task.status === "queued" && (
          <Tooltip>
            <TooltipTrigger
              render={<Button variant="ghost" size="icon-sm" onClick={() => onCancel(task.id)} />}
            >
              <Ban />
            </TooltipTrigger>
            <TooltipContent>取消排队</TooltipContent>
          </Tooltip>
        )}
      </div>
    </div>
  )
}

export function BatchPanel({ batch, onBatchChange }: BatchPanelProps) {
  const [expanded, setExpanded] = useState(true)
  const finished =
    batch.counts.succeeded + batch.counts.failed + batch.counts.canceled +
    batch.counts.skipped + batch.counts.interrupted
  const progress = batch.counts.total ? Math.round((finished / batch.counts.total) * 100) : 0
  const retryable = batch.counts.failed + batch.counts.interrupted

  const retry = async (id: string) => {
    try {
      onBatchChange(await api.retryTask(id))
    } catch (error) {
      toast.error(errorMessage(error))
    }
  }
  const cancel = async (id: string) => {
    try {
      await api.cancelTask(id)
    } catch (error) {
      toast.error(errorMessage(error))
    }
  }

  return (
    <section className="border-b bg-white">
      <div className="flex min-h-16 items-center gap-4 px-6 py-3">
        <div className="min-w-0 flex-1">
          <div className="mb-2 flex items-center gap-3">
            <p className="text-sm font-medium">最近导入批次</p>
            <Badge variant="secondary">淘宝</Badge>
            <span className="text-xs text-muted-foreground">
              {formatDateTime(batch.createdAt)}
            </span>
          </div>
          <div className="flex items-center gap-3">
            <Progress value={progress} className="h-1.5 max-w-md" />
            <span className="w-28 text-xs tabular-nums text-muted-foreground">
              {finished}/{batch.counts.total} · {progress}%
            </span>
          </div>
        </div>
        <div className="hidden items-center gap-3 text-xs text-muted-foreground xl:flex">
          <span>处理中 {batch.counts.processing}</span>
          <span>成功 {batch.counts.succeeded}</span>
          <span>失败 {batch.counts.failed}</span>
          <span>跳过 {batch.counts.skipped}</span>
        </div>
        {retryable > 0 && (
          <Button
            variant="outline"
            size="sm"
            onClick={async () => {
              try {
                onBatchChange(await api.retryFailedBatch(batch.id))
              } catch (error) {
                toast.error(errorMessage(error))
              }
            }}
          >
            <RefreshCw />
            重试失败项
          </Button>
        )}
        {batch.counts.queued > 0 && (
          <Button
            variant="outline"
            size="sm"
            onClick={async () => {
              try {
                await api.cancelBatch(batch.id)
              } catch (error) {
                toast.error(errorMessage(error))
              }
            }}
          >
            <Ban />
            取消排队
          </Button>
        )}
        <Tooltip>
          <TooltipTrigger
            render={<Button variant="ghost" size="icon-sm" onClick={() => setExpanded(!expanded)} />}
          >
            {expanded ? <ChevronUp /> : <ChevronDown />}
          </TooltipTrigger>
          <TooltipContent>{expanded ? "收起任务" : "展开任务"}</TooltipContent>
        </Tooltip>
      </div>
      {expanded && (
        <div className="border-t bg-zinc-50/50">
          {batch.status === "completed_with_errors" && (
            <Alert className="mx-4 mt-3 rounded-md border-amber-200 bg-amber-50 text-amber-900">
              <AlertCircle />
              <AlertDescription>部分图片未完成，可查看错误后重试。</AlertDescription>
            </Alert>
          )}
          <ScrollArea className="max-h-56">
            {batch.tasks.map((task) => (
              <TaskRow key={task.id} task={task} onRetry={retry} onCancel={cancel} />
            ))}
          </ScrollArea>
        </div>
      )}
    </section>
  )
}
