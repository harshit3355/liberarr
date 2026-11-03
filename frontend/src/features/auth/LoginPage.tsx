import { useState, type FormEvent } from 'react'
import { useNavigate } from 'react-router'
import { api, ApiError } from '../../shared/api'

export function LoginPage() {
  const navigate = useNavigate()
  const [username, setUsername] = useState('admin')
  const [password, setPassword] = useState('')
  const [error, setError] = useState<string | null>(null)

  async function submit(e: FormEvent) {
    e.preventDefault()
    setError(null)
    try {
      await api('/api/auth/login', { method: 'POST', body: JSON.stringify({ username, password }) })
      navigate('/')
    } catch (err) {
      const status = err instanceof ApiError ? err.status : 0
      setError(status === 429 ? 'Trop de tentatives, réessaie dans 15 minutes.' : 'Identifiants incorrects.')
    }
  }

  return (
    <main className="grid min-h-screen place-items-center px-4">
      <form onSubmit={submit} className="w-full max-w-sm space-y-4 rounded-xl border border-zinc-800 bg-zinc-900 p-6">
        <h1 className="text-xl font-semibold">Liberarr</h1>
        <label className="block text-sm">
          Utilisateur
          <input
            className="mt-1 w-full rounded-md border border-zinc-700 bg-zinc-950 px-3 py-2"
            value={username}
            onChange={(e) => setUsername(e.target.value)}
            autoComplete="username"
          />
        </label>
        <label className="block text-sm">
          Mot de passe
          <input
            type="password"
            className="mt-1 w-full rounded-md border border-zinc-700 bg-zinc-950 px-3 py-2"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            autoComplete="current-password"
          />
        </label>
        {error && <p role="alert" className="text-sm text-red-400">{error}</p>}
        <button className="w-full rounded-md bg-emerald-600 px-3 py-2 font-medium hover:bg-emerald-500">Se connecter</button>
      </form>
    </main>
  )
}
