export interface Segment {
  start: number
  end: number
  text: string
}

export interface Transcript {
  engine: string
  language: string | null
  text: string
  data: { segments: Segment[]; words: { word: string; start: number; end: number }[] }
}
