import { formatTimestamp } from '../../shared/time'
import { useToggleAction, type Action } from '../recordings/api'

interface Props {
  recordingId: number
  actions: Action[]
  onSeek: (seconds: number) => void
}

export function ActionsTable({ recordingId, actions, onSeek }: Props) {
  const toggle = useToggleAction(recordingId)

  if (!actions.length) return <p className="text-sm text-zinc-400">Aucune action.</p>
  return (
    <div className="overflow-x-auto">
      {toggle.isError && (
        <p role="alert" className="mb-2 text-sm text-red-400">
          Impossible de mettre à jour l'action.
        </p>
      )}
      <table className="w-full text-sm">
        <thead className="text-left text-zinc-300">
          <tr>
            <th className="p-2">Fait</th>
            <th className="p-2">Action</th>
            <th className="p-2">Responsable</th>
            <th className="p-2">Échéance</th>
            <th className="p-2">Moment</th>
          </tr>
        </thead>
        <tbody>
          {actions.map((a, index) => (
            <tr key={index} className="border-t border-zinc-800">
              <td className="p-2">
                <input
                  type="checkbox"
                  aria-label={`Marquer « ${a.action} » comme faite`}
                  checked={a.done}
                  onChange={(e) => toggle.mutate({ index, done: e.target.checked })}
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
