<script setup lang="ts">
import { useTheme, type ThemeChoice } from '@/stores/theme'

const theme = useTheme()
const options: { value: ThemeChoice; label: string; title: string }[] = [
  { value: 'system', label: 'Auto', title: 'Follow the system setting' },
  { value: 'light', label: '☀', title: 'Light' },
  { value: 'dark', label: '☾', title: 'Dark' },
]
</script>

<template>
  <div
    class="inline-flex overflow-hidden rounded border border-edge text-xs"
    role="radiogroup"
    aria-label="Theme"
    data-testid="theme-chooser"
  >
    <button
      v-for="o in options"
      :key="o.value"
      type="button"
      role="radio"
      :aria-checked="theme.choice === o.value"
      :title="o.title"
      class="px-2 py-1"
      :class="
        theme.choice === o.value
          ? 'bg-accent text-white'
          : 'bg-surface text-muted hover:bg-surface-2'
      "
      @click="theme.set(o.value)"
    >
      {{ o.label }}
    </button>
  </div>
</template>
