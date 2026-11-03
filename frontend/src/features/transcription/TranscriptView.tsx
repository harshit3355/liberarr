import { formatTimestamp } from '../../shared/api'
import type { Segment } from './types'

interface Props {
  segments: Segment[]
  currentTime: number
  onSeek: (seconds: number) => void
}

export function TranscriptView({ segments, currentTime, onSeek }: Props) {
  return (
    <ol className="space-y-1">
      {segments.map((s, i) => {
        const active = currentTime >= s.start && currentTime < s.end
        return (
          <li key={i}>
            <button
              onClick={() => onSeek(s.start)}
              className={`w-full rounded px-2 py-1 text-left hover:bg-zinc-800 ${active ? 'bg-emerald-950 text-emerald-100' : ''}`}
            >
              <span className="mr-2 font-mono text-xs text-zinc-500">{formatTimestamp(s.start)}</span>
              {s.text}
            </button>
          </li>
        )
      })}
    </ol>
  )
}
