import { useRef } from 'react'
import { useTranslation } from 'react-i18next'
import Markdown from 'react-markdown'
import { Link, useParams } from 'react-router'
import { ActionsTable } from '../summaries/ActionsTable'
import { Mindmap } from '../summaries/Mindmap'
import { TranscriptView } from '../transcription/TranscriptView'
import { useRecording, useRerun } from './api'

export function RecordingPage() {
  const { t } = useTranslation()
  const id = Number(useParams().id)
  const { data: rec, isLoading, isError } = useRecording(id)
  const rerun = useRerun(id)
  const audio = useRef<HTMLAudioElement>(null)

  const seek = (seconds: number) => {
    if (!audio.current) return
    audio.current.currentTime = seconds
    void audio.current.play().catch(() => {})
  }

  if (isLoading) return <p className="p-8 text-zinc-400">{t('common.loading')}</p>
  if (isError || !rec)
    return (
      <p role="alert" className="p-8 text-red-400">
        {t('recordings.notFound')}
      </p>
    )

  const failedStep = rec.status === 'failed' ? rec.failed_step : null
  const failedJob = failedStep ? rec.jobs.findLast((j) => j.step === failedStep) : undefined
  const h2 = 'mb-3 text-lg font-semibold'

  return (
    <main className="mx-auto max-w-5xl space-y-8 px-4 py-8">
      <div>
        <Link to="/" className="text-sm text-zinc-300 underline-offset-2 hover:underline">
          {t('recordings.back')}
        </Link>
        <h1 className="mt-2 text-2xl font-semibold">
          {rec.summary?.title ?? rec.title ?? t('recordings.untitled', { id: rec.id })}
        </h1>
      </div>

      {failedStep && (
        <div
          role="alert"
          className="flex flex-wrap items-center gap-3 rounded-lg border border-red-800 bg-red-950 p-4 text-sm"
        >
          <span className="flex-1">
            {t(failedJob?.error ? 'recordings.stepFailedWithError' : 'recordings.stepFailed', {
              step: t(`recordings.steps.${failedStep}`, { defaultValue: failedStep }),
              error: failedJob?.error,
            })}
          </span>
          <button
            onClick={() => rerun.mutate(failedStep)}
            disabled={rerun.isPending}
            className="rounded-md bg-red-700 px-3 py-1.5 font-medium hover:bg-red-600 disabled:opacity-60"
          >
            {t('recordings.retry')}
          </button>
        </div>
      )}
      {rerun.isError && (
        <p role="alert" className="text-sm text-red-400">
          {t('recordings.retryError')}
        </p>
      )}
      {rec.status === 'processing' && (
        <p role="status" className="text-sm text-sky-300">
          {t('recordings.processing')}
        </p>
      )}

      {rec.no_speech && (
        <p role="status" className="text-sm text-zinc-300">
          {t('recordings.noSpeechDetail')}
        </p>
      )}

      <audio
        ref={audio}
        controls
        src={`/api/recordings/${id}/audio`}
        className="sticky top-2 z-10 w-full"
      />

      {rec.summary && (
        <>
          <section>
            <h2 className={h2}>{t('summaries.summary')}</h2>
            {/* No raw HTML (react-markdown default) and no images: a remote URL in LLM output would leak. */}
            <div className="space-y-2 [&_a]:text-emerald-400 [&_a]:underline [&_h1]:mt-4 [&_h1]:text-xl [&_h1]:font-semibold [&_h2]:mt-4 [&_h2]:font-semibold [&_h3]:mt-3 [&_h3]:font-semibold [&_li]:ml-5 [&_ol]:list-decimal [&_ul]:list-disc">
              <Markdown disallowedElements={['img']}>{rec.summary.summary_md}</Markdown>
            </div>
          </section>
          <section>
            <h2 className={h2}>{t('summaries.mindmap')}</h2>
            <Mindmap code={rec.summary.mindmap} />
          </section>
          <section>
            <h2 className={h2}>{t('summaries.actions.title')}</h2>
            <ActionsTable recordingId={id} actions={rec.summary.actions} onSeek={seek} />
          </section>
        </>
      )}

      {rec.transcript && (
        <section>
          <h2 className={h2}>{t('transcription.title')}</h2>
          <TranscriptView transcript={rec.transcript} audio={audio} onSeek={seek} />
        </section>
      )}
    </main>
  )
}
