import { useEffect, useId, useState } from 'react'
import mermaid from 'mermaid'

// strict: the diagram comes from an LLM, never let it run scripts or raw HTML.
mermaid.initialize({ startOnLoad: false, securityLevel: 'strict', theme: 'dark' })

export function Mindmap({ code }: { code: string }) {
  const id = useId().replace(/:/g, '')
  const [svg, setSvg] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    let alive = true
    mermaid
      .render(`mm-${id}`, code)
      .then((r) => alive && (setSvg(r.svg), setError(null)))
      .catch((e: Error) => alive && (setSvg(null), setError(e.message)))
    return () => {
      alive = false
    }
  }, [code, id])

  if (error)
    return (
      <div className="space-y-2">
        <p role="alert" className="text-sm text-amber-400">Carte mentale illisible : {error}</p>
        <pre className="overflow-x-auto rounded bg-zinc-900 p-3 text-xs">{code}</pre>
      </div>
    )
  return svg ? <div className="overflow-x-auto" dangerouslySetInnerHTML={{ __html: svg }} /> : null
}
