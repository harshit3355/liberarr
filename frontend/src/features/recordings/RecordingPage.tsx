import { useRef, useState } from 'react'
import Markdown from 'react-markdown'
import { Link, useParams } from 'react-router'
import { ActionsTable } from '../summaries/ActionsTable'
import { Mindmap } from '../summaries/Mindmap'
import { TranscriptView } from '../transcription/TranscriptView'
import { useRecording, useRerun } from './api'

const STEP_LABEL: Record<string, string> = { transcribe: 'transcription', summarize: 'résumé', export: 'export Obsidian' }

export function RecordingPage() {
  const id = Number(useParams().id)
  const { data: rec, isLoading } = useRecording(id)
  const rerun = useRerun(id)
  const audio = useRef<HTMLAudioElement>(null)
  const [time, setTime] = useState(0)

  const seek = (s: number) => {
    if (!audio.current) return
    audio.current.currentTime = s
    void audio.current.play()
  }

  if (isLoading || !rec) return <p className="p-8 text-zinc-500">Chargement…</p>
  const failed = rec.jobs.at(-1)?.status === 'failed' ? rec.jobs.at(-1) : undefined

  return (
    <main className="mx-auto max-w-5xl space-y-8 px-4 py-8">
      <div>
        <Link to="/" className="text-sm text-zinc-400 hover:underline">← Enregistrements</Link>
        <h1 className="mt-2 text-2xl font-semibold">{rec.title ?? `Enregistrement ${rec.id}`}</h1>
      </div>

      {failed && (
        <div role="alert" className="flex flex-wrap items-center gap-3 rounded-lg border border-red-800 bg-red-950 p-4 text-sm">
          <span className="flex-1">
            Échec de l'étape {STEP_LABEL[failed.step] ?? failed.step} : {failed.error}
          </span>
          <button
            onClick={() => rerun.mutate(failed.step)}
            className="rounded-md bg-red-700 px-3 py-1.5 font-medium hover:bg-red-600"
          >
            Relancer
          </button>
        </div>
      )}
      {rec.status === 'processing' && <p className="text-sm text-sky-300">Traitement en cours…</p>}

      <audio
        ref={audio}
        controls
        src={`/api/recordings/${id}/audio`}
        onTimeUpdate={(e) => setTime(e.currentTarget.currentTime)}
        className="sticky top-2 z-10 w-full"
      />

      {rec.summary && (
        <>
          <section>
            <h2 className="mb-3 text-lg font-semibold">Résumé</h2>
            <div className="prose-sm space-y-2 [&_h2]:mt-4 [&_h2]:font-semibold [&_ul]:list-disc [&_ul]:pl-5">
              <Markdown>{rec.summary.summary_md}</Markdown>
            </div>
          </section>
          <section>
            <h2 className="mb-3 text-lg font-semibold">Carte mentale</h2>
            <Mindmap code={rec.summary.mindmap} />
          </section>
          <section>
            <h2 className="mb-3 text-lg font-semibold">Actions</h2>
            <ActionsTable recordingId={id} actions={rec.summary.actions} onSeek={seek} />
          </section>
        </>
      )}

      {rec.transcript && (
        <section>
          <h2 className="mb-3 text-lg font-semibold">Transcription</h2>
          <TranscriptView segments={rec.transcript.data.segments} currentTime={time} onSeek={seek} />
        </section>
      )}
    </main>
  )
}
