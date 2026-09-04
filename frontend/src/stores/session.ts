import { defineStore } from 'pinia'
import { ref } from 'vue'
import { ApiError, api } from '@/api/client'
import type { Me } from '@/api/types'

export const useSession = defineStore('session', () => {
  const me = ref<Me | null>(null)
  const checked = ref(false)

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

  return { me, checked, load, logout }
})
