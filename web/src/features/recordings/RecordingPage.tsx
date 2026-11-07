import { useRef, useState } from 'react'
import Markdown from 'react-markdown'
import { Link, useParams } from 'react-router'
import { ActionsTable } from '../summaries/ActionsTable'
import { Mindmap } from '../summaries/Mindmap'
import { TranscriptView } from '../transcription/TranscriptView'
import { useRecording, useRerun } from './api'

const STEP_LABEL: Record<string, string> = {
  transcribe: 'transcription',
  summarize: 'résumé',
  export: 'export',
}

export function RecordingPage() {
  const id = Number(useParams().id)
  const { data: rec, isLoading, isError } = useRecording(id)
  const rerun = useRerun(id)
  const audio = useRef<HTMLAudioElement>(null)
  const [time, setTime] = useState(0)

  const seek = (seconds: number) => {
    if (!audio.current) return
    audio.current.currentTime = seconds
    void audio.current.play().catch(() => {})
  }

  if (isLoading) return <p className="p-8 text-zinc-400">Chargement…</p>
  if (isError || !rec)
    return (
      <p role="alert" className="p-8 text-red-400">
        Enregistrement introuvable.
      </p>
    )

  const failedStep = rec.status === 'failed' ? rec.failed_step : null
  const failedJob = failedStep ? rec.jobs.findLast((j) => j.step === failedStep) : undefined
  const h2 = 'mb-3 text-lg font-semibold'

  return (
    <main className="mx-auto max-w-5xl space-y-8 px-4 py-8">
      <div>
        <Link to="/" className="text-sm text-zinc-300 underline-offset-2 hover:underline">
          ← Enregistrements
        </Link>
        <h1 className="mt-2 text-2xl font-semibold">{rec.summary?.title ?? rec.title ?? `Enregistrement ${rec.id}`}</h1>
      </div>

      {failedStep && (
        <div
          role="alert"
          className="flex flex-wrap items-center gap-3 rounded-lg border border-red-800 bg-red-950 p-4 text-sm"
        >
          <span className="flex-1">
            Échec de l'étape « {STEP_LABEL[failedStep] ?? failedStep} »
            {failedJob?.error ? ` : ${failedJob.error}` : '.'}
          </span>
          <button
            onClick={() => rerun.mutate(failedStep)}
            disabled={rerun.isPending}
            className="rounded-md bg-red-700 px-3 py-1.5 font-medium hover:bg-red-600 disabled:opacity-60"
          >
            Relancer
          </button>
        </div>
      )}
      {rerun.isError && (
        <p role="alert" className="text-sm text-red-400">
          Impossible de relancer l'étape.
        </p>
      )}
      {rec.status === 'processing' && (
        <p role="status" className="text-sm text-sky-300">
          Traitement en cours…
        </p>
      )}

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
            <h2 className={h2}>Résumé</h2>
            {/* react-markdown renders no raw HTML by default. */}
            <div className="space-y-2 [&_h2]:mt-4 [&_h2]:font-semibold [&_li]:ml-5 [&_ul]:list-disc">
              <Markdown>{rec.summary.summary_md}</Markdown>
            </div>
          </section>
          <section>
            <h2 className={h2}>Carte mentale</h2>
            <Mindmap code={rec.summary.mindmap} />
          </section>
          <section>
            <h2 className={h2}>Actions</h2>
            <ActionsTable recordingId={id} actions={rec.summary.actions} onSeek={seek} />
          </section>
        </>
      )}

      {rec.transcript && (
        <section>
          <h2 className={h2}>Transcription</h2>
          <TranscriptView transcript={rec.transcript} currentTime={time} onSeek={seek} />
        </section>
      )}
    </main>
  )
}
