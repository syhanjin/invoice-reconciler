import { Badge } from "@/components/ui/badge"
import type { LedgerMatchStatus } from "@/lib/types"
import { cn } from "@/lib/utils"

const labels: Record<LedgerMatchStatus, string> = {
  unmatched: "未匹配",
  matched: "已匹配",
  confirmed: "已确认",
}

export function LedgerStatusBadge({ status }: { status: LedgerMatchStatus }) {
  return (
    <Badge
      variant="outline"
      className={cn(
        status === "unmatched" && "border-red-200 bg-red-50 text-red-800",
        status === "matched" && "border-amber-200 bg-amber-50 text-amber-800",
        status === "confirmed" && "border-emerald-200 bg-emerald-50 text-emerald-800",
      )}
    >
      {labels[status]}
    </Badge>
  )
}

export function ledgerRowClass(status: LedgerMatchStatus, selected: boolean) {
  if (selected) return undefined
  if (status === "confirmed") return "bg-emerald-50/60 hover:bg-emerald-50"
  if (status === "matched") return "bg-amber-50/60 hover:bg-amber-50"
  return "bg-red-50/40 hover:bg-red-50/70"
}
