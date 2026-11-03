import { useMutation, useQueryClient } from '@tanstack/react-query'
import { api, formatTimestamp } from '../../shared/api'
import type { Action } from './types'

interface Props {
  recordingId: number
  actions: Action[]
  onSeek: (seconds: number) => void
}

export function ActionsTable({ recordingId, actions, onSeek }: Props) {
  const qc = useQueryClient()
  const toggle = useMutation({
    mutationFn: ({ index, done }: { index: number; done: boolean }) =>
      api(`/api/recordings/${recordingId}/actions/${index}`, { method: 'PATCH', body: JSON.stringify({ done }) }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['recordings', recordingId] }),
  })

  if (!actions.length) return <p className="text-sm text-zinc-500">Aucune action.</p>
  return (
    <div className="overflow-x-auto">
      <table className="w-full text-sm">
        <thead className="text-left text-zinc-400">
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
              <td className={`p-2 ${a.done ? 'text-zinc-500 line-through' : ''}`}>{a.action}</td>
              <td className="p-2">{a.owner ?? '—'}</td>
              <td className="p-2">{a.due ?? '—'}</td>
              <td className="p-2">
                {a.start != null && (
                  <button className="font-mono text-xs text-emerald-400 hover:underline" onClick={() => onSeek(a.start!)}>
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
