import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import { ApiError, api } from '@/api/client'
import type { Me } from '@/api/types'
import { dayIn, systemZone } from '@/lib/time'

export const useSession = defineStore('session', () => {
  const me = ref<Me | null>(null)
  const checked = ref(false)
  // A tab is often left open across the day boundary, so the day is worked
  // out from the clock rather than kept as the server said it was at login.
  const now = ref(Date.now())
  if (typeof window !== 'undefined') {
    setInterval(() => (now.value = Date.now()), 60_000)
    window.addEventListener('visibilitychange', () => (now.value = Date.now()))
    window.addEventListener('focus', () => (now.value = Date.now()))
  }

  async function load(): Promise<Me | null> {
    try {
      me.value = await api.me()
    } catch (e) {
      if (e instanceof ApiError && e.status === 401) me.value = null
      else throw e
    } finally {
      checked.value = true
    }
    return me.value
  }

  async function logout() {
    await api.logout()
    me.value = null
  }

  const members = computed(() => me.value?.household?.members ?? [])

  /** Today on the person's own clock and day boundary, ticking. */
  const today = computed(() =>
    me.value
      ? dayIn(me.value.timezone, me.value.user.day_boundary_minutes, new Date(now.value))
      : dayIn(systemZone(), 0, new Date(now.value)),
  )

  return { me, checked, members, today, load, logout }
})
