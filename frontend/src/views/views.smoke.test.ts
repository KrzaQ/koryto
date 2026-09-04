// Every view mounts against fixture API responses without console errors.
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'
import DayView from './DayView.vue'
import LoginView from './LoginView.vue'

function json(data: unknown) {
  return new Response(JSON.stringify(data), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  })
}

function fakeFetch(input: RequestInfo | URL): Promise<Response> {
  const url = String(input)
  const path = url.split('?')[0]!
  if (path === '/api/me') return Promise.resolve(json({ name: 'Dev', email: null }))
  return Promise.resolve(
    new Response(
      JSON.stringify({ error: { code: 'not_found', message: `no fixture for ${url}` } }),
      { status: 404 },
    ),
  )
}

describe('views', () => {
  let errors: unknown[][]
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.stubGlobal('fetch', vi.fn(fakeFetch))
    errors = []
    vi.spyOn(console, 'error').mockImplementation((...args) => errors.push(args))
    vi.spyOn(console, 'warn').mockImplementation((...args) => errors.push(args))
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
    return w
  }

  it('DayView', async () => {
    const w = await render(DayView)
    expect(w.text()).toContain('Today')
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
