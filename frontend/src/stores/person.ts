import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import { useSession } from './session'

const KEY = 'koryto.person'

function stored(): number | null {
  try {
    const v = Number(localStorage.getItem(KEY))
    return Number.isInteger(v) && v > 0 ? v : null
  } catch {
    return null
  }
}

/** Whose data the views show: a household member, me by default. */
export const usePerson = defineStore('person', () => {
  const session = useSession()
  const chosen = ref<number | null>(stored())

  const id = computed(() => {
    const me = session.me?.user.id
    const c = chosen.value
    if (c !== null && session.members.some((m) => m.id === c)) return c
    return me ?? 0
  })
  const isMe = computed(() => id.value === session.me?.user.id)
  const member = computed(() => session.members.find((m) => m.id === id.value))
  const name = computed(() => member.value?.name ?? member.value?.email ?? 'me')

  function set(userId: number) {
    chosen.value = userId
    try {
      localStorage.setItem(KEY, String(userId))
    } catch {
      /* ignore */
    }
  }

  return { id, isMe, member, name, set }
})
