import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api } from '../../shared/api'

export type Status = 'processing' | 'failed' | 'done'

export interface RecordingItem {
  id: number
  title: string | null
  source: 'device' | 'file'
  duration_s: number | null
  created_at: string
  status: Status
  failed_step: string | null
  no_speech: boolean
}

export interface Segment {
  start: number
  end: number
  text: string
}

export interface Transcript {
  engine: string
  language: string | null
  text: string
  has_timestamps: boolean
  segments: Segment[]
}

export interface Action {
  action: string
  owner: string | null
  due: string | null
  start: number | null
  done: boolean
}

export interface Summary {
  title: string
  summary_md: string
  mindmap: string
  actions: Action[]
}

export interface Job {
  id: number
  step: string
  status: 'queued' | 'running' | 'failed' | 'done'
  attempts: number
  error: string | null
  updated_at: string
}

export interface RecordingDetail extends RecordingItem {
  transcript: Transcript | null
  summary: Summary | null
  jobs: Job[]
}

const POLL_MS = 5000

export const useRecordings = () =>
  useQuery({
    queryKey: ['recordings'],
    queryFn: () => api<RecordingItem[]>('/api/recordings'),
    refetchInterval: (q) => (q.state.data?.some((r) => r.status === 'processing') ? POLL_MS : false),
  })

export const useRecording = (id: number) =>
  useQuery({
    queryKey: ['recordings', id],
    queryFn: () => api<RecordingDetail>(`/api/recordings/${id}`),
    refetchInterval: (q) => (q.state.data?.status === 'processing' ? POLL_MS : false),
  })

export function useRerun(id: number) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (step: string) => api(`/api/recordings/${id}/rerun`, { method: 'POST', body: JSON.stringify({ step }) }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['recordings'] }),
  })
}

export function useToggleAction(id: number) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ index, done }: { index: number; done: boolean }) =>
      api<Action[]>(`/api/recordings/${id}/actions/${index}`, { method: 'PATCH', body: JSON.stringify({ done }) }),
    // The PATCH returns the whole updated array: put it straight into the cached detail.
    onSuccess: (actions) =>
      qc.setQueryData<RecordingDetail>(['recordings', id], (d) =>
        d?.summary ? { ...d, summary: { ...d.summary, actions } } : d,
      ),
  })
}
