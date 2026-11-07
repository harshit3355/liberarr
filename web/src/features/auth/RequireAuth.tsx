import { Link, Navigate, Outlet } from 'react-router'
import { useLogout, useMe } from './api'

/** Gate and shell for every page except login. */
export function RequireAuth() {
  const me = useMe()
  const logout = useLogout()

  if (me.isLoading) return <p className="p-8 text-zinc-400">Chargement…</p>
  if (!me.data) return <Navigate to="/login" replace />

  return (
    <>
      <header className="border-b border-zinc-800">
        <div className="mx-auto flex max-w-5xl items-center justify-between px-4 py-3">
          <Link to="/" className="font-semibold">
            Liberarr
          </Link>
          <button
            onClick={() => logout.mutate()}
            className="rounded-md px-2 py-1 text-sm text-zinc-300 hover:bg-zinc-800"
          >
            Se déconnecter
          </button>
        </div>
      </header>
      <Outlet />
    </>
  )
}
