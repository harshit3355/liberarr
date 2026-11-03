import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api } from '../../shared/api'
import type { Transcript } from '../transcription/types'
import type { Summary } from '../summaries/types'

export type Status = 'processing' | 'failed' | 'done'

export interface RecordingItem {
  id: number
  title: string | null
  source: string
  duration_s: number | null
  created_at: string
  status: Status
  failed_step: string | null
}

export interface Job {
  id: number
  step: string
  status: string
  attempts: number
  error: string | null
}

export interface RecordingDetail extends RecordingItem {
  transcript: Transcript | null
  summary: Summary | null
  jobs: Job[]
}

const poll = (busy: boolean) => (busy ? 5000 : false)

export function useRecordings() {
  return useQuery({
    queryKey: ['recordings'],
    queryFn: () => api<RecordingItem[]>('/api/recordings'),
    refetchInterval: (q) => poll(!!q.state.data?.some((r) => r.status === 'processing')),
  })
}

export function useRecording(id: number) {
  return useQuery({
    queryKey: ['recordings', id],
    queryFn: () => api<RecordingDetail>(`/api/recordings/${id}`),
    refetchInterval: (q) => poll(q.state.data?.status === 'processing'),
  })
}

export function useUpload() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (file: File) => {
      const body = new FormData()
      body.append('file', file)
      return api<{ id: number }>('/api/recordings', { method: 'POST', body })
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: ['recordings'] }),
  })
}

export function useRerun(id: number) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (step: string) => api(`/api/recordings/${id}/rerun`, { method: 'POST', body: JSON.stringify({ step }) }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['recordings'] }),
  })
}
