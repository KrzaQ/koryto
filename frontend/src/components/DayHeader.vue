<script setup lang="ts">
// The day at a glance: how much room is left today, how the last week went,
// protein, sport. Room is measured against what the body burns (base plus
// the day's sport) when an estimate exists, else against the target.
import { computed } from 'vue'
import type { DayDto, Summary } from '@/api/types'
import { roomOf, weekRoomOf } from '@/lib/budget'
import { formatMinutes, signed } from '@/lib/units'

const props = defineProps<{ day: DayDto; week: Summary | null; isToday: boolean }>()

const target = computed(() => props.day.target?.kcal ?? null)
const burn = computed(() => props.day.expenditure)
const burnKcal = computed(() => burn.value.kcal ?? null)
const eaten = computed(() => props.day.totals.kcal)

const against = computed(() => roomOf(props.day))
const room = computed(() => against.value?.kcal ?? null)
const over = computed(() => room.value !== null && room.value < 0)
const pct = computed(() =>
  against.value ? Math.min(100, Math.round((eaten.value / against.value.against) * 100)) : 0,
)
const vsTarget = computed(() => (target.value !== null ? target.value - eaten.value : null))

const weekRoom = computed(() => weekRoomOf(props.week))
</script>

<template>
  <div class="grid gap-4 md:grid-cols-5" data-testid="day-header">
    <div class="card p-4 md:col-span-2" data-testid="budget-card">
      <div class="text-xs tracking-wide text-muted uppercase">
        Budget{{ isToday ? ' today' : '' }}
      </div>
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
      <div class="mt-2 text-xs text-muted tabular-nums" data-testid="budget-note">
        <template v-if="against?.kind === 'burn'">
          {{ burnKcal }} burnt = {{ burn.base_kcal }} base<template v-if="burn.sport_kcal">
            + {{ burn.sport_kcal }} sport</template
          >
        </template>
        <template v-if="vsTarget !== null">
          <template v-if="against?.kind === 'burn'"> · </template>{{ target }} target ·
          <span :class="vsTarget < 0 ? 'text-danger' : ''" data-testid="balance"
            >{{ Math.abs(vsTarget) }} {{ vsTarget < 0 ? 'over' : 'under' }}</span
          >
        </template>
        <div v-if="burn.basis === 'seed'" class="mt-1">
          Seed estimate: {{ burn.logged_days }}/14 logged days, {{ burn.weight_span_days }}/10
          weigh-in days.
        </div>
        <div v-else-if="burn.basis === 'none'" class="mt-1">
          No burn yet: needs a weigh-in and height, birth date and sex on the
          <RouterLink to="/profile" class="link">profile</RouterLink>.
        </div>
      </div>
    </div>
    <div class="card p-4" data-testid="week-card">
      <div class="text-xs tracking-wide text-muted uppercase">
        {{ isToday ? 'Last 7 days' : 'The 7 days to it' }}
      </div>
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
          Per logged day vs {{ weekRoom.kind }} · {{ week.logged_days }}/7 logged<template
            v-if="week.mean_kcal != null"
          >
            · {{ week.mean_kcal }} eaten/day</template
          ><template v-if="week.sport_kcal"> · {{ week.sport_kcal }} sport</template>
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
        <template v-if="day.totals.sport_kcal">In the day's burn.</template>
        <template v-else-if="day.totals.sport_minutes">No kcal logged: no effect.</template>
        <template v-else>Sport kcal add to the burn.</template>
      </div>
    </div>
  </div>
</template>
