import { defineStore } from 'pinia'
import { ref } from 'vue'

export type ThemeChoice = 'system' | 'light' | 'dark'
const KEY = 'koryto.theme'

function stored(): ThemeChoice {
  try {
    const v = localStorage.getItem(KEY)
    if (v === 'light' || v === 'dark' || v === 'system') return v
  } catch {
    /* private mode or blocked storage */
  }
  return 'system'
}

function systemDark(): boolean {
  return (
    typeof window !== 'undefined' && !!window.matchMedia?.('(prefers-color-scheme: dark)').matches
  )
}

export const useTheme = defineStore('theme', () => {
  const choice = ref<ThemeChoice>(stored())
  const resolved = ref<'light' | 'dark'>('light')

  function apply() {
    resolved.value = choice.value === 'system' ? (systemDark() ? 'dark' : 'light') : choice.value
    document.documentElement.dataset.theme = resolved.value
  }

  function set(c: ThemeChoice) {
    choice.value = c
    apply()
    try {
      localStorage.setItem(KEY, c)
    } catch {
      /* ignore */
    }
  }

  apply()
  if (typeof window !== 'undefined' && window.matchMedia) {
    window.matchMedia('(prefers-color-scheme: dark)').addEventListener?.('change', () => {
      if (choice.value === 'system') apply()
    })
  }

  return { choice, resolved, set }
})
