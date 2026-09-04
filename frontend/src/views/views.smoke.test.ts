// Every view mounts against fixture API responses without console errors.
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'
import { useSession } from '@/stores/session'
import { usePerson } from '@/stores/person'
import DayView from './DayView.vue'
import FoodsView from './FoodsView.vue'
import LoginView from './LoginView.vue'
import ProfileView from './ProfileView.vue'
import TokensView from './TokensView.vue'
import TrendsView from './TrendsView.vue'

// jsdom has neither canvas nor ResizeObserver; the chart component is replaced.
vi.mock('vue-echarts', () => ({
  default: { name: 'VChart', props: ['option'], template: '<div class="chart-stub" />' },
}))

const me = {
  kind: 'session',
  user: {
    id: 1,
    name: 'Alice',
    email: 'alice@example.com',
    household_id: 1,
    day_boundary_minutes: 240,
    height_mm: 1700,
    born_on: '1990-06-15',
    sex: 'female',
    activity_factor: '1.40',
  },
  household: {
    id: 1,
    name: 'home',
    members: [
      { id: 1, name: 'Alice', email: 'alice@example.com' },
      { id: 2, name: 'Bob', email: 'bob@example.com' },
    ],
  },
  timezone: 'Europe/Warsaw',
  today: '2026-09-04',
  target: {
    id: 1,
    user_id: 1,
    valid_from: '2026-09-01',
    kcal: 1800,
    protein_g: 120,
    weight_kg: '75',
  },
  scopes: ['read', 'write', 'edit'],
  can_write: true,
  can_edit: true,
}
const day = {
  day: '2026-09-04',
  user_id: 1,
  logged: true,
  meals: [
    {
      id: 10,
      user_id: 1,
      eaten_at: '2026-09-04T06:00:00Z',
      timezone: 'Europe/Warsaw',
      day: '2026-09-04',
      day_override: false,
      description: 'Eggs on toast',
      kcal: 420,
      protein_g: 22,
      source: 'estimate',
      food_id: null,
      portions: null,
      created_by: 1,
      created_via: 'mcp',
      voided: false,
    },
    {
      id: 11,
      user_id: 1,
      eaten_at: '2026-09-04T17:30:00Z',
      timezone: 'Europe/Warsaw',
      day: '2026-09-04',
      day_override: false,
      description: 'Lentil curry',
      kcal: 780,
      protein_g: null,
      source: 'food',
      food_id: 5,
      portions: '1.5',
      created_by: 2,
      created_via: 'web',
      voided: false,
    },
  ],
  weights: [
    {
      id: 20,
      user_id: 1,
      measured_at: '2026-09-04T05:00:00Z',
      timezone: 'Europe/Warsaw',
      day: '2026-09-04',
      day_override: false,
      weight_kg: '82.4',
      weight_g: 82400,
      created_by: 1,
      created_via: 'mcp',
      voided: false,
    },
  ],
  activities: [
    {
      id: 30,
      user_id: 1,
      started_at: '2026-09-04T15:00:00Z',
      timezone: 'Europe/Warsaw',
      day: '2026-09-04',
      day_override: false,
      kind: 'run',
      minutes: 90,
      duration: '1h30',
      kcal: 600,
      note: '',
      created_by: 1,
      created_via: 'web',
      voided: false,
    },
  ],
  totals: { kcal: 1200, protein_g: 22, meals: 2, meals_without_protein: 1, sport_minutes: 90 },
  target: me.target,
  balance: -600,
  expenditure: { kcal: 2218, basis: 'seed', logged_days: 3, weight_span_days: 2, seed_kcal: 2218 },
  balance_vs_expenditure: -1018,
}
const foods = [
  {
    id: 5,
    name: 'Lentil curry',
    aliases: ['dal'],
    portion: '1 bowl (350 g)',
    kcal: 520,
    protein_g: 24,
    created_by: 1,
    created_at: '2026-09-01T00:00:00Z',
    updated_at: '2026-09-01T00:00:00Z',
    archived_at: null,
    uses: 3,
  },
]
const tokens = [
  {
    id: 1,
    name: 'claude-code',
    scopes: ['read', 'write', 'edit'],
    user_id: 1,
    created_by: 1,
    created_at: '2026-08-01T10:00:00Z',
    last_used_at: null,
    revoked_at: null,
  },
  {
    id: 2,
    name: 'openwebui',
    scopes: ['read', 'write', 'delegate'],
    user_id: null,
    created_by: 1,
    created_at: '2026-08-01T10:00:00Z',
    last_used_at: '2026-09-04T10:00:00Z',
    revoked_at: null,
  },
]

function json(data: unknown) {
  return new Response(JSON.stringify(data), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  })
}

const calls: string[] = []
function fakeFetch(input: RequestInfo | URL, init?: RequestInit): Promise<Response> {
  const url = String(input)
  const path = url.split('?')[0]!
  calls.push(`${init?.method ?? 'GET'} ${url}`)
  if (path === '/api/me') return Promise.resolve(json(me))
  if (path === '/api/day') return Promise.resolve(json(day))
  if (path === '/api/foods') return Promise.resolve(json(foods))
  if (path === '/api/tokens') return Promise.resolve(json(tokens))
  if (path === '/api/users/1/targets') return Promise.resolve(json([me.target]))
  if (path === '/api/users/1/locations')
    return Promise.resolve(
      json([
        {
          id: 1,
          user_id: 1,
          valid_from: '0001-01-01T00:00:00Z',
          timezone: 'Europe/Warsaw',
          origin: true,
        },
        {
          id: 2,
          user_id: 1,
          valid_from: '2026-09-10T12:00:00Z',
          timezone: 'America/New_York',
          origin: false,
        },
      ]),
    )
  if (path === '/api/meals' && init?.method === 'POST') return Promise.resolve(json([day.meals[0]]))
  if (path === '/api/days')
    return Promise.resolve(
      json({
        user_id: 1,
        from: '2026-09-01',
        to: '2026-09-04',
        days: [
          {
            day: '2026-09-01',
            logged: false,
            kcal: null,
            protein_g: null,
            meals: 0,
            meals_without_protein: 0,
            sport_minutes: 0,
            weight_g: null,
            trend_g: null,
            target_kcal: 1800,
            balance: null,
          },
          {
            day: '2026-09-02',
            logged: true,
            kcal: 1900,
            protein_g: 100,
            meals: 3,
            meals_without_protein: 0,
            sport_minutes: 0,
            weight_g: 82500,
            trend_g: 82500,
            target_kcal: 1800,
            balance: 100,
          },
          {
            day: '2026-09-03',
            logged: true,
            kcal: 1700,
            protein_g: 90,
            meals: 2,
            meals_without_protein: 1,
            sport_minutes: 45,
            weight_g: null,
            trend_g: null,
            target_kcal: 1800,
            balance: -100,
          },
          {
            day: '2026-09-04',
            logged: true,
            kcal: 1200,
            protein_g: 22,
            meals: 2,
            meals_without_protein: 1,
            sport_minutes: 90,
            weight_g: 82400,
            trend_g: 82490,
            target_kcal: 1800,
            balance: -600,
          },
        ],
      }),
    )
  if (path === '/api/stats/weight')
    return Promise.resolve(
      json({
        user_id: 1,
        from: '2026-09-01',
        to: '2026-09-04',
        goal_g: 75000,
        points: [
          { day: '2026-09-02', weight_g: 82500, trend_g: 82500 },
          { day: '2026-09-04', weight_g: 82400, trend_g: 82490 },
        ],
      }),
    )
  if (path === '/api/stats/expenditure')
    return Promise.resolve(
      json({
        user_id: 1,
        from: '2026-09-01',
        to: '2026-09-04',
        latest: { kcal: 2218, basis: 'seed', logged_days: 3, weight_span_days: 2, seed_kcal: 2218 },
        days: ['2026-09-01', '2026-09-02', '2026-09-03', '2026-09-04'].map((day) => ({
          day,
          kcal: 2218,
          basis: 'seed',
          logged_days: 3,
        })),
      }),
    )
  if (path === '/api/stats/weekly')
    return Promise.resolve(
      json({
        user_id: 1,
        from: '2026-09-01',
        to: '2026-09-04',
        weeks: [
          {
            week: '2026-W36',
            start: '2026-08-31',
            days: 4,
            logged_days: 3,
            mean_kcal: 1600,
            total_kcal: 4800,
            sport_minutes: 135,
            mean_balance_vs_target: -200,
            mean_expenditure: 2218,
            mean_balance_vs_expenditure: -618,
          },
        ],
      }),
    )
  return Promise.resolve(
    new Response(
      JSON.stringify({ error: { code: 'not_found', message: `no fixture for ${url}` } }),
      {
        status: 404,
      },
    ),
  )
}

describe('views', () => {
  let errors: unknown[][]
  beforeEach(async () => {
    setActivePinia(createPinia())
    vi.stubGlobal('fetch', vi.fn(fakeFetch))
    calls.length = 0
    errors = []
    vi.spyOn(console, 'error').mockImplementation((...args) => errors.push(args))
    vi.spyOn(console, 'warn').mockImplementation((...args) => errors.push(args))
    await useSession().load()
  })
  afterEach(() => {
    vi.restoreAllMocks()
    vi.unstubAllGlobals()
  })

  async function render(component: unknown, props: Record<string, unknown> = {}, path = '/') {
    const router = createRouter({
      history: createMemoryHistory(),
      routes: [{ path: '/:pathMatch(.*)*', component: { template: '<div />' } }],
    })
    await router.push(path)
    const w = mount(component as never, { props, global: { plugins: [router] } })
    await flushPromises()
    await flushPromises()
    return w
  }

  it('DayView shows the day and opens the add row', async () => {
    const w = await render(DayView, { day: '2026-09-04' })
    expect(w.find('[data-testid="day-title"]').text()).toContain('4 Sept 2026')
    expect(w.find('[data-testid="day-title"]').text()).toContain('today')
    expect(w.findAll('[data-testid="meal-row"]')).toHaveLength(2)
    expect(w.text()).toContain('Lentil curry')
    expect(w.text()).toContain('× 1.5')
    expect(w.find('[data-testid="balance"]').text()).toBe('−600')
    expect(w.find('[data-testid="balance-vs-expenditure"]').text()).toBe('−1018')
    expect(w.find('[data-testid="expenditure-card"]').text()).toContain('2218')
    expect(w.findAll('[data-testid="weight-row"]')).toHaveLength(1)
    expect(w.findAll('[data-testid="activity-row"]')).toHaveLength(1)
    expect(w.text()).toContain('1 meal without protein')
    expect(calls.some((c) => c.startsWith('GET /api/day?user=1&date=2026-09-04'))).toBe(true)

    await w.find('[data-testid="add-meal"]').trigger('click')
    const editor = w.find('[data-testid="meal-editor"]')
    expect(editor.exists()).toBe(true)
    // Both members are offered for a shared meal, me ticked.
    expect(editor.findAll('input[type="checkbox"]')).toHaveLength(2)
    await editor.find('[data-testid="editor-description"]').setValue('Soup')
    await editor.find('[data-testid="editor-kcal"]').setValue('250')
    await w.find('[data-testid="editor-save"]').trigger('click')
    await flushPromises()
    const post = calls.find((c) => c.startsWith('POST /api/meals'))
    expect(post).toBeDefined()
    expect(errors).toEqual([])
  })

  it('DayView follows the person chooser', async () => {
    usePerson().set(2)
    const w = await render(DayView, { day: '2026-09-04' })
    expect(calls.some((c) => c.startsWith('GET /api/day?user=2&date=2026-09-04'))).toBe(true)
    expect(w.find('[data-testid="person-note"]').text()).toBe('Bob')
    usePerson().set(1)
    expect(errors).toEqual([])
  })

  it('FoodsView', async () => {
    const w = await render(FoodsView)
    expect(w.findAll('[data-testid="food-row"]')).toHaveLength(1)
    expect(w.text()).toContain('dal')
    await w.find('[data-testid="add-food"]').trigger('click')
    expect(w.find('[data-testid="food-editor"]').exists()).toBe(true)
    expect(w.find('[data-testid="food-save"]').attributes('disabled')).toBeDefined()
    expect(errors).toEqual([])
  })

  it('ProfileView', async () => {
    const w = await render(ProfileView)
    expect(w.find('[data-testid="profile-person"]').text()).toBe('Alice')
    expect(w.findAll('[data-testid="target-row"]')).toHaveLength(1)
    expect(w.findAll('[data-testid="location-row"]')).toHaveLength(2)
    expect(w.text()).toContain('origin')
    expect(w.text()).toContain('America/New_York')
    const form = w.find('[data-testid="profile-form"]')
    expect((form.find('input[type="time"]').element as HTMLInputElement).value).toBe('04:00')
    await w.find('[data-testid="add-target"]').trigger('click')
    expect(w.find('[data-testid="target-editor"]').exists()).toBe(true)
    expect(errors).toEqual([])
  })

  it('TokensView', async () => {
    const w = await render(TokensView)
    expect(w.findAll('[data-testid="token-row"]')).toHaveLength(2)
    expect(w.text()).toContain('delegate')
    expect(w.text()).toContain('Alice')
    expect(errors).toEqual([])
  })

  it('TrendsView', async () => {
    const w = await render(TrendsView)
    expect(w.find('[data-testid="trend-tiles"]').text()).toContain('2218')
    expect(w.find('[data-testid="trend-tiles"]').text()).toContain('82.49')
    expect(w.find('[data-testid="trend-tiles"]').text()).toContain('3')
    for (const id of [
      'weight-chart',
      'intake-chart',
      'expenditure-chart',
      'balance-chart',
      'sport-chart',
      'calendar-chart',
    ])
      expect(w.find(`[data-testid="${id}"]`).exists(), id).toBe(true)
    await w.find('input[type="checkbox"]').setValue(true)
    expect(w.find('[data-testid="trends-table"]').text()).toContain('2026-W36')
    expect(w.find('[data-testid="trends-table"]').text()).toContain('−618')
    await w.find('[data-testid="balance-mode"] button:last-child').trigger('click')
    expect(calls.some((c) => c.startsWith('GET /api/stats/weekly?user=1'))).toBe(true)
    expect(errors).toEqual([])
  })

  it('LoginView keeps next local', async () => {
    const w = await render(LoginView, {}, '/login?next=//evil')
    expect(w.find('[data-testid="login-button"]').attributes('href')).toBe(
      '/api/auth/login?next=%2F',
    )
    expect(errors).toEqual([])
  })
})
