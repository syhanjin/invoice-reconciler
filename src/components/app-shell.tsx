import { ClipboardList, ReceiptText, Settings } from "lucide-react"
import type { ReactNode } from "react"

import { Button } from "@/components/ui/button"
import { Separator } from "@/components/ui/separator"
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip"
import { cn } from "@/lib/utils"
import type { NavigationView } from "@/lib/types"

interface AppShellProps {
  view: NavigationView
  onViewChange: (view: NavigationView) => void
  children: ReactNode
}

const navigation = [
  { id: "orders" as const, label: "订单", icon: ClipboardList },
  { id: "settings" as const, label: "设置", icon: Settings },
]

export function AppShell({ view, onViewChange, children }: AppShellProps) {
  return (
    <div className="flex min-h-screen bg-[#f5f5f3] text-foreground">
      <aside className="flex w-56 shrink-0 flex-col border-r bg-white px-3 py-4">
        <div className="flex h-11 items-center gap-3 px-2">
          <div className="flex size-8 items-center justify-center rounded-md bg-zinc-900 text-white">
            <ReceiptText className="size-4" />
          </div>
          <div className="min-w-0">
            <p className="truncate text-sm font-semibold">票据助手</p>
            <p className="truncate text-xs text-muted-foreground">订单识别工作台</p>
          </div>
        </div>
        <Separator className="my-4" />
        <nav className="space-y-1">
          {navigation.map((item) => {
            const Icon = item.icon
            return (
              <Button
                key={item.id}
                variant="ghost"
                className={cn(
                  "h-9 w-full justify-start gap-3 px-3 font-normal",
                  view === item.id && "bg-zinc-100 font-medium",
                )}
                onClick={() => onViewChange(item.id)}
              >
                <Icon className="size-4" />
                {item.label}
              </Button>
            )
          })}
        </nav>
        <div className="mt-auto px-2 pb-1">
          <Tooltip>
            <TooltipTrigger
              render={<div className="flex items-center gap-2 text-xs text-muted-foreground" />}
            >
                <span className="size-2 rounded-full bg-emerald-500" />
                本地数据
            </TooltipTrigger>
            <TooltipContent side="right">订单和截图仅保存在本机</TooltipContent>
          </Tooltip>
        </div>
      </aside>
      <main className="min-w-0 flex-1 overflow-x-hidden">{children}</main>
    </div>
  )
}
