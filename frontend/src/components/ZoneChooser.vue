<script setup lang="ts">
import { computed } from 'vue'
import { systemZone, zoneAbbr, zoneCity } from '@/lib/time'
import { useTimezone, type ZoneChoice } from '@/stores/timezone'

const tz = useTimezone()
const options = computed<{ value: ZoneChoice; label: string; title: string }[]>(() => [
  {
    value: 'mine',
    label: zoneCity(tz.mine),
    title: `Where I am: ${tz.mine} (${zoneAbbr(tz.mine)})`,
  },
  {
    value: 'system',
    label: 'Local',
    title: `This device: ${systemZone()} (${zoneAbbr(systemZone())})`,
  },
])
</script>

<template>
  <div
    v-if="tz.mine !== systemZone()"
    class="inline-flex overflow-hidden rounded border border-edge text-xs"
    role="radiogroup"
    aria-label="Time zone"
    data-testid="zone-chooser"
  >
    <button
      v-for="o in options"
      :key="o.value"
      type="button"
      role="radio"
      :aria-checked="tz.choice === o.value"
      :title="o.title"
      class="px-2 py-1"
      :class="
        tz.choice === o.value ? 'bg-accent text-white' : 'bg-surface text-muted hover:bg-surface-2'
      "
      @click="tz.set(o.value)"
    >
      {{ o.label }}
    </button>
  </div>
</template>
