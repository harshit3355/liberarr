import { useRef, useState, type DragEvent } from 'react'
import { Link } from 'react-router'
import { formatTimestamp } from '../../shared/api'
import { useRecordings, useUpload, type Status } from './api'

const BADGE: Record<Status, string> = {
  processing: 'bg-sky-900 text-sky-200',
  failed: 'bg-red-900 text-red-200',
  done: 'bg-emerald-900 text-emerald-200',
}
const LABEL: Record<Status, string> = { processing: 'En cours', failed: 'Échec', done: 'Prêt' }

export function RecordingsPage() {
  const recordings = useRecordings()
  const upload = useUpload()
  const input = useRef<HTMLInputElement>(null)
  const [dragging, setDragging] = useState(false)

  const send = (files: FileList | null) => Array.from(files ?? []).forEach((f) => upload.mutate(f))
  const onDrop = (e: DragEvent) => {
    e.preventDefault()
    setDragging(false)
    send(e.dataTransfer.files)
  }

  return (
    <main
      className="mx-auto max-w-5xl px-4 py-8"
      onDragOver={(e) => (e.preventDefault(), setDragging(true))}
      onDragLeave={() => setDragging(false)}
      onDrop={onDrop}
    >
      <header className="mb-6 flex items-center justify-between gap-4">
        <h1 className="text-2xl font-semibold">Enregistrements</h1>
        <button
          onClick={() => input.current?.click()}
          className="rounded-md bg-emerald-600 px-3 py-2 text-sm font-medium hover:bg-emerald-500"
        >
          Importer un audio
        </button>
        <input ref={input} type="file" accept="audio/*" multiple hidden onChange={(e) => send(e.target.files)} />
      </header>
      {dragging && (
        <div className="mb-4 rounded-xl border-2 border-dashed border-emerald-600 p-8 text-center text-emerald-300">
          Dépose tes fichiers audio ici
        </div>
      )}
      {upload.isError && <p role="alert" className="mb-4 text-sm text-red-400">Import refusé : {upload.error.message}</p>}
      {recordings.isLoading && <p className="text-zinc-500">Chargement…</p>}
      {recordings.data?.length === 0 && (
        <p className="text-zinc-500">Aucun enregistrement. Importe un fichier ou dépose-le dans le dossier inbox.</p>
      )}
      <ul className="divide-y divide-zinc-800 rounded-xl border border-zinc-800">
        {recordings.data?.map((r) => (
          <li key={r.id}>
            <Link to={`/recordings/${r.id}`} className="flex items-center gap-4 px-4 py-3 hover:bg-zinc-900">
              <span className="flex-1 truncate">{r.title ?? `Enregistrement ${r.id}`}</span>
              <span className="text-sm text-zinc-500">{new Date(r.created_at.replace(' ', 'T') + 'Z').toLocaleString('fr-FR')}</span>
              <span className="w-16 text-right font-mono text-sm text-zinc-400">
                {r.duration_s != null ? formatTimestamp(r.duration_s) : '—'}
              </span>
              <span className={`rounded-full px-2 py-0.5 text-xs ${BADGE[r.status]}`}>{LABEL[r.status]}</span>
            </Link>
          </li>
        ))}
      </ul>
    </main>
  )
}
