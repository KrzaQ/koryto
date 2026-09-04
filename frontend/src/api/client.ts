// Same-origin JSON client. Every failure becomes an ApiError carrying the
// server's error envelope, so views can branch on `code`.

import type { Me } from './types'

export class ApiError extends Error {
  constructor(
    public status: number,
    public code: string,
    message: string,
  ) {
    super(message)
  }
}

type Query = Record<string, string | number | boolean | undefined>

function url(path: string, query?: Query): string {
  const params = new URLSearchParams()
  for (const [k, v] of Object.entries(query ?? {})) {
    if (v !== undefined && v !== '') params.set(k, String(v))
  }
  const q = params.toString()
  return q ? `${path}?${q}` : path
}

export async function request<T>(
  method: string,
  path: string,
  body?: unknown,
  query?: Query,
): Promise<T> {
  const init: RequestInit = { method, headers: {}, credentials: 'same-origin' }
  if (body !== undefined) {
    init.headers = { 'content-type': 'application/json' }
    init.body = JSON.stringify(body)
  }
  const res = await fetch(url(path, query), init)
  if (res.status === 204) return undefined as T
  const text = await res.text()
  let data: unknown = null
  if (text) {
    try {
      data = JSON.parse(text)
    } catch {
      data = text
    }
  }
  if (!res.ok) {
    const env = data as { error?: { code?: string; message?: string } } | null
    throw new ApiError(
      res.status,
      env?.error?.code ?? 'error',
      env?.error?.message ?? res.statusText,
    )
  }
  return data as T
}

export const api = {
  me: () => request<Me>('GET', '/api/me'),
  logout: () => request<void>('POST', '/api/auth/logout'),
}
