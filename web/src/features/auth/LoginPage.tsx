import type { TFunction } from 'i18next'
import { useState, type FormEvent } from 'react'
import { useTranslation } from 'react-i18next'
import { Navigate } from 'react-router'
import { ApiError } from '../../shared/api'
import { useLogin, useMe } from './api'

function message(e: Error, t: TFunction): string {
  if (e instanceof ApiError && e.status === 401) return t('auth.errors.invalidCredentials')
  if (e instanceof ApiError && e.status === 429) return t('auth.errors.tooManyAttempts')
  return t('auth.errors.generic')
}

export function LoginPage() {
  const { t } = useTranslation()
  const me = useMe()
  const login = useLogin()
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')

  if (me.data) return <Navigate to="/" replace />

  const submit = (e: FormEvent) => {
    e.preventDefault()
    login.mutate({ username, password })
  }
  const field = 'mt-1 w-full rounded-md border border-zinc-700 bg-zinc-900 px-3 py-2 focus:border-emerald-500'

  return (
    <main className="mx-auto flex min-h-screen max-w-sm flex-col justify-center px-4">
      <h1 className="mb-6 text-2xl font-semibold">Liberarr</h1>
      <form onSubmit={submit} className="space-y-4">
        <label className="block text-sm text-zinc-300">
          {t('auth.username')}
          <input
            className={field}
            autoComplete="username"
            required
            value={username}
            onChange={(e) => setUsername(e.target.value)}
          />
        </label>
        <label className="block text-sm text-zinc-300">
          {t('auth.password')}
          <input
            className={field}
            type="password"
            autoComplete="current-password"
            required
            value={password}
            onChange={(e) => setPassword(e.target.value)}
          />
        </label>
        {login.isError && (
          <p role="alert" className="text-sm text-red-400">
            {message(login.error, t)}
          </p>
        )}
        <button
          type="submit"
          disabled={login.isPending}
          className="w-full rounded-md bg-emerald-600 px-3 py-2 text-sm font-medium hover:bg-emerald-500 disabled:opacity-60"
        >
          {t('auth.signIn')}
        </button>
      </form>
    </main>
  )
}
