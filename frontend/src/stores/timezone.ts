import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import { DEFAULT_HOUSE_ZONE, systemZone } from '@/lib/time'
import { useSession } from './session'

export type ZoneChoice = 'mine' | 'system'
const KEY = 'koryto.timezone'

function stored(): ZoneChoice {
  try {
    return localStorage.getItem(KEY) === 'system' ? 'system' : 'mine'
  } catch {
    return 'mine'
  }
}

/** Which clock times are shown and typed on: where I am now, or the browser's. */
export const useTimezone = defineStore('timezone', () => {
  const session = useSession()
  const choice = ref<ZoneChoice>(stored())
  const mine = computed(() => session.me?.timezone ?? DEFAULT_HOUSE_ZONE)
  const zone = computed(() => (choice.value === 'system' ? systemZone() : mine.value))

  function set(c: ZoneChoice) {
    choice.value = c
    try {
      localStorage.setItem(KEY, c)
    } catch {
      /* ignore */
    }
  }

  return { choice, mine, zone, set }
})
