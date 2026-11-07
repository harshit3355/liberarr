export interface Action {
  action: string
  owner: string | null
  due: string | null
  start: number | null
  done: boolean
}

export interface Summary {
  template: string
  title: string | null
  summary_md: string
  mindmap: string
  actions: Action[]
}
