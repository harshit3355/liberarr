import { formatTimestamp } from '../../shared/time'
import type { Transcript } from '../recordings/api'

interface Props {
  transcript: Transcript
  currentTime: number
  onSeek: (seconds: number) => void
}

export function TranscriptView({ transcript, currentTime, onSeek }: Props) {
  if (!transcript.has_timestamps)
    return (
      <div className="space-y-3">
        <p className="text-sm text-zinc-400">Ce fournisseur ne renvoie pas d'horodatage</p>
        <p className="whitespace-pre-wrap">{transcript.text}</p>
      </div>
    )

  return (
    <ol className="space-y-1">
      {transcript.segments.map((s, i) => {
        const active = currentTime >= s.start && currentTime < s.end
        return (
          <li key={i}>
            <button
              onClick={() => onSeek(s.start)}
              aria-current={active || undefined}
              className={`w-full rounded px-2 py-1 text-left hover:bg-zinc-800 ${active ? 'bg-emerald-950 text-emerald-100' : ''}`}
            >
              <span className="mr-2 font-mono text-xs text-zinc-400">{formatTimestamp(s.start)}</span>
              {s.text}
            </button>
          </li>
        )
      })}
    </ol>
  )
}
