export class ApiError extends Error {
  status: number
  constructor(status: number, message: string) {
    super(message)
    this.status = status
  }
}

/** JSON fetch against the same-origin API; the session cookie travels automatically. */
export async function api<T = void>(path: string, init: RequestInit = {}): Promise<T> {
  const headers = init.body ? { 'Content-Type': 'application/json', ...init.headers } : init.headers
  const res = await fetch(path, { credentials: 'same-origin', ...init, headers })
  // Login and the session probe handle their own 401; anywhere else the session has expired.
  if (res.status === 401 && !path.startsWith('/api/auth/')) window.location.assign('/login')
  if (!res.ok) {
    const body = (await res.json().catch(() => ({}))) as { detail?: string }
    throw new ApiError(res.status, body.detail ?? res.statusText)
  }
  // 204 and 202 (rerun) carry no body.
  const text = await res.text()
  return (text ? JSON.parse(text) : undefined) as T
}
