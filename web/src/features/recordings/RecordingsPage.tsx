import { Link } from 'react-router'
import { formatDate, formatTimestamp } from '../../shared/time'
import { useRecordings, type Status } from './api'

const BADGE: Record<Status, string> = {
  processing: 'bg-sky-900 text-sky-100',
  failed: 'bg-red-900 text-red-100',
  done: 'bg-emerald-900 text-emerald-100',
}
const LABEL: Record<Status, string> = { processing: 'En cours', failed: 'Échec', done: 'Prêt' }

export function RecordingsPage() {
  const { data, isLoading, isError } = useRecordings()

  return (
    <main className="mx-auto max-w-5xl px-4 py-8">
      <h1 className="mb-6 text-2xl font-semibold">Enregistrements</h1>
      {isLoading && <p className="text-zinc-400">Chargement…</p>}
      {isError && (
        <p role="alert" className="text-red-400">
          Impossible de charger les enregistrements.
        </p>
      )}
      {data?.length === 0 && <p className="text-zinc-400">Aucun enregistrement pour le moment.</p>}
      {data && data.length > 0 && (
        <ul className="divide-y divide-zinc-800 rounded-xl border border-zinc-800">
          {data.map((r) => (
            <li key={r.id}>
              <Link
                to={`/recordings/${r.id}`}
                className="flex flex-wrap items-center gap-x-4 gap-y-1 px-4 py-3 hover:bg-zinc-900"
              >
                <span className="min-w-0 flex-1 truncate">{r.title ?? `Enregistrement ${r.id}`}</span>
                <time dateTime={r.created_at} className="text-sm text-zinc-400">
                  {formatDate(r.created_at)}
                </time>
                <span className="w-16 text-right font-mono text-sm text-zinc-300">
                  {r.duration_s != null ? formatTimestamp(r.duration_s) : '—'}
                </span>
                <span className={`rounded-full px-2 py-0.5 text-xs ${BADGE[r.status]}`}>{LABEL[r.status]}</span>
              </Link>
            </li>
          ))}
        </ul>
      )}
    </main>
  )
}
