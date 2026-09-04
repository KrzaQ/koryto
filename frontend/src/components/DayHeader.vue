<script setup lang="ts">
// The day at a glance: how much room is left today, how the last week went,
// protein, sport. Room is measured against what the body burns (base plus
// the day's sport) when an estimate exists, else against the target.
import { computed, ref, watch } from 'vue'
import { api } from '@/api/client'
import type { DayDto, Summary } from '@/api/types'
import { shiftDay } from '@/lib/day'
import { formatMinutes, signed } from '@/lib/units'

const props = defineProps<{ day: DayDto }>()

const target = computed(() => props.day.target?.kcal ?? null)
const burn = computed(() => props.day.expenditure)
const burnKcal = computed(() => burn.value.kcal ?? null)
const eaten = computed(() => props.day.totals.kcal)

/** What the budget is measured against: the burn when known, else the target. */
const against = computed<{ kind: 'burn' | 'target'; kcal: number } | null>(() => {
  if (burnKcal.value !== null) return { kind: 'burn', kcal: burnKcal.value }
  if (target.value !== null) return { kind: 'target', kcal: target.value }
  return null
})
const room = computed(() => (against.value ? against.value.kcal - eaten.value : null))
const over = computed(() => room.value !== null && room.value < 0)
const pct = computed(() =>
  against.value ? Math.min(100, Math.round((eaten.value / against.value.kcal) * 100)) : 0,
)
const vsTarget = computed(() => (target.value !== null ? target.value - eaten.value : null))

const week = ref<Summary | null>(null)
async function loadWeek() {
  try {
    week.value = await api.stats.summary({
      user: props.day.user_id,
      from: shiftDay(props.day.day, -6),
      to: props.day.day,
    })
  } catch {
    week.value = null
  }
}
watch(() => [props.day.day, props.day.user_id, props.day.totals.kcal], loadWeek, {
  immediate: true,
})
const weekRoom = computed(() => {
  const w = week.value
  if (!w) return null
  if (w.mean_balance_vs_expenditure != null)
    return { kind: 'burn' as const, kcal: -w.mean_balance_vs_expenditure }
  if (w.mean_balance != null) return { kind: 'target' as const, kcal: -w.mean_balance }
  return null
})
</script>

<template>
  <div class="grid gap-4 md:grid-cols-5" data-testid="day-header">
    <div class="card p-4 md:col-span-2" data-testid="budget-card">
      <div class="text-xs tracking-wide text-muted uppercase">Budget today</div>
      <div class="mt-1 flex items-baseline gap-2">
        <template v-if="room !== null">
          <span
            class="text-2xl font-semibold tabular-nums"
            :class="over ? 'text-danger' : 'text-ok'"
            data-testid="room"
            >{{ Math.abs(room) }}</span
          >
          <span class="text-sm text-muted">kcal {{ over ? 'over' : 'left' }}</span>
        </template>
        <template v-else>
          <span class="text-2xl font-semibold tabular-nums">—</span>
        </template>
        <span class="ml-auto text-sm text-muted tabular-nums" data-testid="eaten"
          >{{ eaten }} eaten</span
        >
      </div>
      <div v-if="against" class="mt-2 h-2 overflow-hidden rounded bg-surface-3">
        <div
          class="h-full rounded"
          :class="over ? 'bg-danger' : 'bg-accent'"
          :style="{ width: `${pct}%` }"
        ></div>
      </div>
      <div class="mt-2 text-xs text-muted" data-testid="budget-note">
        <template v-if="against?.kind === 'burn'">
          Of {{ burnKcal }} burnt: {{ burn.base_kcal }} base<template v-if="burn.sport_kcal">
            + {{ burn.sport_kcal }} sport</template
          >.
          <template v-if="vsTarget !== null">
            Target {{ target }}:
            <span :class="vsTarget < 0 ? 'text-danger' : ''" data-testid="balance"
              >{{ Math.abs(vsTarget) }} {{ vsTarget < 0 ? 'over' : 'under' }}</span
            >.
          </template>
          <template v-if="burn.basis === 'seed'"
            >The base is the Mifflin-St Jeor seed until {{ burn.logged_days }}/14 logged days and
            {{ burn.weight_span_days }}/10 weigh-in days.</template
          >
        </template>
        <template v-else-if="against?.kind === 'target'">
          Target {{ target }}:
          <span data-testid="balance"
            >{{ Math.abs(vsTarget!) }} {{ vsTarget! < 0 ? 'over' : 'under' }}</span
          >. What you burn is unknown: it needs a weigh-in and height, birth date and sex on the
          <RouterLink to="/profile" class="link">profile</RouterLink>.
        </template>
        <template v-else
          >No target and no expenditure estimate yet. Set a target or fill in the
          <RouterLink to="/profile" class="link">profile</RouterLink> and log a weigh-in.</template
        >
      </div>
    </div>
    <div class="card p-4" data-testid="week-card">
      <div class="text-xs tracking-wide text-muted uppercase">Last 7 days</div>
      <div class="mt-1 flex items-baseline gap-2">
        <template v-if="weekRoom">
          <span
            class="text-2xl font-semibold tabular-nums"
            :class="weekRoom.kcal < 0 ? 'text-danger' : 'text-ok'"
            data-testid="week-room"
            >{{ signed(weekRoom.kcal) }}</span
          >
          <span class="text-sm text-muted">kcal/day</span>
        </template>
        <span v-else class="text-2xl font-semibold tabular-nums">—</span>
      </div>
      <div class="mt-2 text-xs text-muted">
        <template v-if="week && weekRoom">
          Room per logged day against the {{ weekRoom.kind === 'burn' ? 'burn' : 'target' }},
          {{ week.logged_days }} of 7 logged<template v-if="week.mean_kcal != null"
            >, {{ week.mean_kcal }} eaten on average</template
          ><template v-if="week.sport_kcal">, {{ week.sport_kcal }} kcal of sport</template>.
        </template>
        <template v-else>Nothing logged in the last week.</template>
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
    <div class="card p-4" data-testid="sport-card">
      <div class="text-xs tracking-wide text-muted uppercase">Sport</div>
      <div class="mt-1 flex items-baseline gap-2">
        <span class="text-2xl font-semibold tabular-nums">{{
          day.totals.sport_minutes ? formatMinutes(day.totals.sport_minutes) : '—'
        }}</span>
        <span v-if="day.totals.sport_kcal" class="text-sm text-muted tabular-nums"
          >{{ day.totals.sport_kcal }} kcal</span
        >
      </div>
      <div class="mt-2 text-xs text-muted">
        <template v-if="day.totals.sport_kcal">Added to today's burn.</template>
        <template v-else-if="day.totals.sport_minutes"
          >Without a kcal figure it does not change the budget.</template
        >
        <template v-else>Sport kcal add to the day's burn.</template>
      </div>
    </div>
  </div>
</template>
