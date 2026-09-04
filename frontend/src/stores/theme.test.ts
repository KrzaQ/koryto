import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, describe, expect, it } from 'vitest'
import { useTheme } from './theme'

describe('theme store', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    localStorage.removeItem('koryto.theme')
  })

  it('stamps the resolved theme on <html> and remembers the choice', () => {
    const t = useTheme()
    expect(['light', 'dark']).toContain(document.documentElement.dataset.theme)
    t.set('dark')
    expect(document.documentElement.dataset.theme).toBe('dark')
    expect(localStorage.getItem('koryto.theme')).toBe('dark')
    t.set('light')
    expect(t.resolved).toBe('light')
    expect(document.documentElement.dataset.theme).toBe('light')
  })
})
