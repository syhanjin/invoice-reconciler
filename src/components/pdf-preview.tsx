import { getDocument, GlobalWorkerOptions } from "pdfjs-dist"
import workerSource from "pdfjs-dist/build/pdf.worker.min.mjs?url"
import { FileWarning, LoaderCircle } from "lucide-react"
import { useEffect, useRef, useState } from "react"

GlobalWorkerOptions.workerSrc = workerSource

export function PdfPreview({ url, label }: { url: string; label: string }) {
  const containerRef = useRef<HTMLDivElement>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState(false)

  useEffect(() => {
    let disposed = false
    const task = getDocument({ url })
    setLoading(true)
    setError(false)
    task.promise.then(async (pdf) => {
      if (disposed || !containerRef.current) return
      containerRef.current.replaceChildren()
      for (let pageNumber = 1; pageNumber <= pdf.numPages; pageNumber += 1) {
        const page = await pdf.getPage(pageNumber)
        if (disposed || !containerRef.current) return
        const base = page.getViewport({ scale: 1 })
        const scale = Math.min(2, 720 / base.width)
        const viewport = page.getViewport({ scale })
        const canvas = document.createElement("canvas")
        const context = canvas.getContext("2d")
        if (!context) continue
        canvas.width = Math.ceil(viewport.width)
        canvas.height = Math.ceil(viewport.height)
        canvas.className = "max-w-full border bg-white shadow-sm"
        canvas.setAttribute("aria-label", `${label} 第 ${pageNumber} 页`)
        containerRef.current.append(canvas)
        await page.render({ canvas, canvasContext: context, viewport }).promise
      }
      if (!disposed) setLoading(false)
    }).catch(() => {
      if (!disposed) {
        setError(true)
        setLoading(false)
      }
    })
    return () => {
      disposed = true
      void task.destroy()
    }
  }, [url])

  return (
    <div className="relative flex min-h-80 items-start justify-center bg-zinc-100 p-4">
      {loading && <LoaderCircle className="absolute top-1/2 size-6 -translate-y-1/2 animate-spin text-muted-foreground" />}
      {error ? (
        <div className="absolute top-1/2 flex -translate-y-1/2 items-center gap-2 text-sm text-muted-foreground"><FileWarning />PDF 预览失败</div>
      ) : (
        <div ref={containerRef} className="flex w-full flex-col items-center gap-4" />
      )}
    </div>
  )
}
