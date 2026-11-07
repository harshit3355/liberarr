import { memo, useEffect, useState, type RefObject } from 'react'
import { useTranslation } from 'react-i18next'
import { formatTimestamp } from '../../shared/time'
import type { Segment, Transcript } from '../recordings/api'

interface Props {
  transcript: Transcript
  audio: RefObject<HTMLAudioElement | null>
  onSeek: (seconds: number) => void
}

// Memoised so a 4 Hz timeupdate re-renders only the rows whose `active` flag flips.
const Row = memo(function Row({ s, active, onSeek }: { s: Segment; active: boolean; onSeek: (seconds: number) => void }) {
  return (
    <li>
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
})

export function TranscriptView({ transcript, audio, onSeek }: Props) {
  const { t } = useTranslation()
  // The playback time lives here, not in the page, so the summary and mind map never re-render on it.
  const [time, setTime] = useState(0)
  useEffect(() => {
    const el = audio.current
    if (!el) return
    const update = () => setTime(el.currentTime)
    el.addEventListener('timeupdate', update)
    return () => el.removeEventListener('timeupdate', update)
  }, [audio])

  if (!transcript.has_timestamps)
    return (
      <div className="space-y-3">
        <p className="text-sm text-zinc-400">{t('transcription.noTimestamps')}</p>
        <p className="whitespace-pre-wrap">{transcript.text}</p>
      </div>
    )

  return (
    <ol className="space-y-1">
      {transcript.segments.map((s, i) => (
        <Row key={i} s={s} active={time >= s.start && time < s.end} onSeek={onSeek} />
      ))}
    </ol>
  )
}
