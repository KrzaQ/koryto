// Same-origin JSON client. Every failure becomes an ApiError carrying the
// server's error envelope, so views can branch on `code`.

import type {
  ActivityDto,
  ActivityInput,
  ActivityPatchInput,
  DayDto,
  DaysDto,
  FoodDto,
  FoodInput,
  FoodPatchInput,
  LocationDto,
  LocationInput,
  LocationPatchInput,
  Me,
  MealDto,
  MealInput,
  MealPatchInput,
  ProfilePatchInput,
  TargetDto,
  TargetInput,
  TargetPatchInput,
  TokenCreated,
  TokenDto,
  TokenInput,
  UserDto,
  WeightDto,
  WeightInput,
  WeightPatchInput,
} from './types'

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

type Range = { user?: number; from: string; to: string; include_voided?: boolean }

export const api = {
  me: () => request<Me>('GET', '/api/me'),
  logout: () => request<void>('POST', '/api/auth/logout'),

  day: (user: number | undefined, date: string, includeVoided = false) =>
    request<DayDto>('GET', '/api/day', undefined, {
      user,
      date,
      include_voided: includeVoided || undefined,
    }),
  days: (r: Range) => request<DaysDto>('GET', '/api/days', undefined, r),

  profile: {
    update: (id: number, patch: ProfilePatchInput) =>
      request<UserDto>('PATCH', `/api/users/${id}/profile`, patch),
  },
  locations: {
    list: (user: number) => request<LocationDto[]>('GET', `/api/users/${user}/locations`),
    create: (user: number, input: LocationInput) =>
      request<LocationDto>('POST', `/api/users/${user}/locations`, input),
    update: (user: number, id: number, patch: LocationPatchInput) =>
      request<LocationDto>('PATCH', `/api/users/${user}/locations/${id}`, patch),
    remove: (user: number, id: number) =>
      request<void>('DELETE', `/api/users/${user}/locations/${id}`),
  },
  targets: {
    list: (user: number) => request<TargetDto[]>('GET', `/api/users/${user}/targets`),
    create: (user: number, input: TargetInput) =>
      request<TargetDto>('POST', `/api/users/${user}/targets`, input),
    update: (user: number, id: number, patch: TargetPatchInput) =>
      request<TargetDto>('PATCH', `/api/users/${user}/targets/${id}`, patch),
    remove: (user: number, id: number) =>
      request<void>('DELETE', `/api/users/${user}/targets/${id}`),
  },
  foods: {
    list: (q = '', includeArchived = false) =>
      request<FoodDto[]>('GET', '/api/foods', undefined, {
        q,
        include_archived: includeArchived || undefined,
      }),
    create: (input: FoodInput) => request<FoodDto>('POST', '/api/foods', input),
    update: (id: number, patch: FoodPatchInput) =>
      request<FoodDto>('PATCH', `/api/foods/${id}`, patch),
    archive: (id: number) => request<FoodDto>('POST', `/api/foods/${id}/archive`),
    unarchive: (id: number) => request<FoodDto>('POST', `/api/foods/${id}/unarchive`),
  },
  meals: {
    list: (r: Range) => request<MealDto[]>('GET', '/api/meals', undefined, r),
    create: (input: MealInput) => request<MealDto[]>('POST', '/api/meals', input),
    update: (id: number, patch: MealPatchInput) =>
      request<MealDto>('PATCH', `/api/meals/${id}`, patch),
    void: (id: number) => request<void>('POST', `/api/meals/${id}/void`),
    unvoid: (id: number) => request<void>('POST', `/api/meals/${id}/unvoid`),
  },
  weights: {
    list: (r: Range) => request<WeightDto[]>('GET', '/api/weights', undefined, r),
    create: (input: WeightInput) => request<WeightDto>('POST', '/api/weights', input),
    update: (id: number, patch: WeightPatchInput) =>
      request<WeightDto>('PATCH', `/api/weights/${id}`, patch),
    void: (id: number) => request<void>('POST', `/api/weights/${id}/void`),
    unvoid: (id: number) => request<void>('POST', `/api/weights/${id}/unvoid`),
  },
  activities: {
    list: (r: Range) => request<ActivityDto[]>('GET', '/api/activities', undefined, r),
    create: (input: ActivityInput) => request<ActivityDto>('POST', '/api/activities', input),
    update: (id: number, patch: ActivityPatchInput) =>
      request<ActivityDto>('PATCH', `/api/activities/${id}`, patch),
    void: (id: number) => request<void>('POST', `/api/activities/${id}/void`),
    unvoid: (id: number) => request<void>('POST', `/api/activities/${id}/unvoid`),
  },
  tokens: {
    list: () => request<TokenDto[]>('GET', '/api/tokens'),
    create: (input: TokenInput) => request<TokenCreated>('POST', '/api/tokens', input),
    revoke: (id: number) => request<void>('DELETE', `/api/tokens/${id}`),
  },
}
