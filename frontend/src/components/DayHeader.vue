<script setup lang="ts">
// The day at a glance: intake against the target, protein, sport.
import { computed } from 'vue'
import type { DayDto } from '@/api/types'
import { formatMinutes, signed } from '@/lib/units'

const props = defineProps<{ day: DayDto }>()

const target = computed(() => props.day.target?.kcal ?? null)
const pct = computed(() =>
  target.value ? Math.min(100, Math.round((props.day.totals.kcal / target.value) * 100)) : 0,
)
const over = computed(() => target.value !== null && props.day.totals.kcal > target.value)
const burn = computed(() => props.day.expenditure)
const overBurn = computed(
  () =>
    props.day.balance_vs_expenditure !== null &&
    props.day.balance_vs_expenditure !== undefined &&
    props.day.balance_vs_expenditure > 0,
)
</script>

<template>
  <div class="grid gap-4 md:grid-cols-5" data-testid="day-header">
    <div class="card p-4 md:col-span-2">
      <div class="text-xs tracking-wide text-muted uppercase">Intake</div>
      <div class="mt-1 flex items-baseline gap-2">
        <span class="text-2xl font-semibold tabular-nums">{{ day.totals.kcal }}</span>
        <span class="text-sm text-muted">kcal</span>
        <span v-if="target !== null" class="text-sm text-muted">of {{ target }}</span>
        <span
          v-if="day.balance !== null && day.balance !== undefined"
          class="ml-auto text-sm tabular-nums"
          :class="over ? 'text-danger' : 'text-ok'"
          data-testid="balance"
          >{{ signed(day.balance) }}</span
        >
      </div>
      <div v-if="target !== null" class="mt-2 h-2 overflow-hidden rounded bg-surface-3">
        <div
          class="h-full rounded"
          :class="over ? 'bg-danger' : 'bg-accent'"
          :style="{ width: `${pct}%` }"
        ></div>
      </div>
      <div v-else class="mt-2 text-xs text-muted">No target set: add one on the profile page.</div>
    </div>
    <div class="card p-4" data-testid="expenditure-card">
      <div class="text-xs tracking-wide text-muted uppercase">Expenditure</div>
      <div class="mt-1 flex items-baseline gap-2">
        <span class="text-2xl font-semibold tabular-nums">{{ burn.kcal ?? '—' }}</span>
        <span v-if="burn.kcal" class="text-sm text-muted">kcal</span>
        <span
          v-if="day.balance_vs_expenditure !== null && day.balance_vs_expenditure !== undefined"
          class="ml-auto text-sm tabular-nums"
          :class="overBurn ? 'text-danger' : 'text-ok'"
          data-testid="balance-vs-expenditure"
          >{{ signed(day.balance_vs_expenditure) }}</span
        >
      </div>
      <div class="mt-2 text-xs text-muted">
        <template v-if="burn.basis === 'adaptive'">From your intake and weight trend.</template>
        <template v-else-if="burn.basis === 'seed'"
          >Mifflin-St Jeor seed until {{ burn.logged_days }}/14 logged days and
          {{ burn.weight_span_days }}/10 weigh-in days.</template
        >
        <template v-else
          >Needs a weigh-in and height, birth date and sex on the
          <RouterLink to="/profile" class="link">profile</RouterLink>.</template
        >
      </div>
    </div>
    <div class="card p-4">
      <div class="text-xs tracking-wide text-muted uppercase">Protein</div>
      <div class="mt-1 flex items-baseline gap-2">
        <span class="text-2xl font-semibold tabular-nums">{{ day.totals.protein_g ?? '—' }}</span>
        <span class="text-sm text-muted">g</span>
        <span v-if="day.target?.protein_g" class="text-sm text-muted"
          >of {{ day.target.protein_g }}</span
        >
      </div>
      <div v-if="day.totals.meals_without_protein > 0" class="mt-2 text-xs text-muted">
        {{ day.totals.meals_without_protein }} meal{{
          day.totals.meals_without_protein === 1 ? '' : 's'
        }}
        without protein
      </div>
    </div>
    <div class="card p-4">
      <div class="text-xs tracking-wide text-muted uppercase">Sport</div>
      <div class="mt-1 flex items-baseline gap-2">
        <span class="text-2xl font-semibold tabular-nums">{{
          day.totals.sport_minutes ? formatMinutes(day.totals.sport_minutes) : '—'
        }}</span>
      </div>
      <div class="mt-2 text-xs text-muted">Never subtracted from intake.</div>
    </div>
  </div>
</template>
