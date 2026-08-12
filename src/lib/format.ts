export function formatMoney(fen: number | null): string {
  if (fen === null) return "-"
  return new Intl.NumberFormat("zh-CN", {
    style: "currency",
    currency: "CNY",
    minimumFractionDigits: 2,
  }).format(fen / 100)
}

export function formatDateTime(value: string | null): string {
  if (!value) return "-"
  if (/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}$/.test(value)) return value
  const date = new Date(value)
  if (Number.isNaN(date.valueOf())) return value
  return new Intl.DateTimeFormat("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  }).format(date)
}

export function parseYuanToFen(value: string): number | null {
  const normalized = value.trim()
  if (!normalized) return null
  if (!/^\d+(?:\.\d{1,2})?$/.test(normalized)) return Number.NaN
  const [yuan, fraction = ""] = normalized.split(".")
  return Number(yuan) * 100 + Number(fraction.padEnd(2, "0"))
}
