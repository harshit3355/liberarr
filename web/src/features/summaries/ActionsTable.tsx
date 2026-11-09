import { useTranslation } from 'react-i18next'
import { ApiError } from '../../shared/api'
import { formatTimestamp } from '../../shared/time'
import { useToggleAction, type Action } from '../recordings/api'

interface Props {
  recordingId: number
  actions: Action[]
  onSeek: (seconds: number) => void
}

export function ActionsTable({ recordingId, actions, onSeek }: Props) {
  const { t } = useTranslation()
  const toggle = useToggleAction(recordingId)

  if (!actions.length) return <p className="text-sm text-zinc-400">{t('summaries.actions.none')}</p>
  return (
    <div className="overflow-x-auto">
      {toggle.isError && (
        <p role="alert" className="mb-2 text-sm text-red-400">
          {t(
            toggle.error instanceof ApiError && toggle.error.status === 409
              ? 'summaries.actions.stale'
              : 'summaries.actions.updateError',
          )}
        </p>
      )}
      <table className="w-full text-sm">
        <thead className="text-left text-zinc-300">
          <tr>
            <th className="p-2">{t('summaries.actions.done')}</th>
            <th className="p-2">{t('summaries.actions.action')}</th>
            <th className="p-2">{t('summaries.actions.owner')}</th>
            <th className="p-2">{t('summaries.actions.due')}</th>
            <th className="p-2">{t('summaries.actions.moment')}</th>
          </tr>
        </thead>
        <tbody>
          {actions.map((a, index) => (
            <tr key={index} className="border-t border-zinc-800">
              <td className="p-2">
                <input
                  type="checkbox"
                  aria-label={t('summaries.actions.markDone', { action: a.action })}
                  checked={a.done}
                  onChange={(e) => toggle.mutate({ index, action: a.action, done: e.target.checked })}
                />
              </td>
              <td className={`p-2 ${a.done ? 'text-zinc-400 line-through' : ''}`}>{a.action}</td>
              <td className="p-2">{a.owner ?? '—'}</td>
              <td className="p-2">{a.due ?? '—'}</td>
              <td className="p-2">
                {a.start != null && (
                  <button
                    className="font-mono text-xs text-emerald-400 hover:underline"
                    onClick={() => onSeek(a.start as number)}
                  >
                    {formatTimestamp(a.start)}
                  </button>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}
