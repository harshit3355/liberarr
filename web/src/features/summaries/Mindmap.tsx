import { useEffect, useId, useState } from 'react'
import { useTranslation } from 'react-i18next'

export function Mindmap({ code }: { code: string }) {
  const { t } = useTranslation()
  const id = `mm-${useId().replace(/[^a-zA-Z0-9]/g, '')}`
  const [svg, setSvg] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    let alive = true
    // Loaded on demand: mermaid is heavy and only the detail page needs it.
    import('mermaid')
      .then(({ default: mermaid }) => {
        // strict: the diagram comes from an LLM, so no scripts and no raw HTML.
        mermaid.initialize({ startOnLoad: false, securityLevel: 'strict', theme: 'dark', suppressErrorRendering: true })
        return mermaid.render(id, code)
      })
      .then((r) => alive && (setSvg(r.svg), setError(null)))
      .catch((e: unknown) => alive && (setSvg(null), setError(e instanceof Error ? e.message : String(e))))
    return () => {
      alive = false
    }
  }, [code, id])

  if (error)
    return (
      <div className="space-y-2">
        <p role="alert" className="text-sm text-amber-400">
          {t('summaries.mindmapError', { error })}
        </p>
        <pre className="overflow-x-auto rounded bg-zinc-900 p-3 text-xs">{code}</pre>
      </div>
    )
  return svg ? <div className="overflow-x-auto" dangerouslySetInnerHTML={{ __html: svg }} /> : null
}
