<script setup lang="ts">
// What's up right now: room left today and yesterday, the last weight, and
// both days' meals and sport. Numbers, not sentences; the day page is one
// click away for editing.
import { computed, onMounted, ref, watch } from 'vue'
import { ApiError, api } from '@/api/client'
import type { DayDto, Summary, WeightStats } from '@/api/types'
import BudgetChart from '@/components/BudgetChart.vue'
import { dayLabel, shiftDay } from '@/lib/day'
import { roomOf } from '@/lib/budget'
import { formatDateTime } from '@/lib/time'
import { formatKg, formatMinutes } from '@/lib/units'
import { usePerson } from '@/stores/person'
import { useSession } from '@/stores/session'
import { useTimezone } from '@/stores/timezone'

const session = useSession()
const person = usePerson()
const tz = useTimezone()

const today = computed(() => session.me?.today ?? new Date().toISOString().slice(0, 10))
const yesterday = computed(() => shiftDay(today.value, -1))

const days = ref<DayDto[]>([])
const weight = ref<WeightStats | null>(null)
const week = ref<Summary | null>(null)
const error = ref<string | null>(null)

async function load() {
  error.value = null
  try {
    const [t, y, w, s] = await Promise.all([
      api.day(person.id, today.value),
      api.day(person.id, yesterday.value),
      api.stats.weight({ user: person.id, from: shiftDay(today.value, -30), to: today.value }),
      api.stats.summary({ user: person.id, from: shiftDay(today.value, -6), to: today.value }),
    ])
    days.value = [t, y]
    weight.value = w
    week.value = s
  } catch (e) {
    error.value = e instanceof ApiError ? e.message : String(e)
  }
}
watch(() => [person.id, today.value], load)
onMounted(load)

const rooms = computed(() => days.value.map(roomOf))
const last = computed(() => weight.value?.points[weight.value.points.length - 1] ?? null)
const weighedAgo = computed(() => {
  if (!last.value) return null
  const n = Math.round(
    (Date.parse(`${today.value}T00:00:00Z`) - Date.parse(`${last.value.day}T00:00:00Z`)) / 86400000,
  )
  return n === 0 ? 'this morning' : n === 1 ? 'yesterday' : `${n} days ago`
})

function time(iso: string) {
  return formatDateTime(iso, tz.zone).slice(11)
}
</script>

<template>
  <main class="mx-auto max-w-6xl space-y-4 px-4 py-6">
    <p v-if="!person.isMe" class="chip" data-testid="person-note">{{ person.name }}</p>
    <p v-if="error" class="note-danger">{{ error }}</p>

    <div v-if="days.length" class="grid gap-4 md:grid-cols-3" data-testid="home-tiles">
      <div
        v-for="(d, i) in days"
        :key="d.day"
        class="card p-4"
        :data-testid="i === 0 ? 'tile-today' : 'tile-yesterday'"
      >
        <div class="text-xs tracking-wide text-muted uppercase">
          {{ i === 0 ? 'Today' : 'Yesterday' }}
        </div>
        <div class="mt-1 flex items-baseline gap-2">
          <template v-if="rooms[i]">
            <span
              class="text-3xl font-semibold tabular-nums"
              :class="rooms[i]!.kcal < 0 ? 'text-danger' : 'text-ok'"
              >{{ Math.abs(rooms[i]!.kcal) }}</span
            >
            <span class="text-sm text-muted">{{ rooms[i]!.kcal < 0 ? 'over' : 'left' }}</span>
          </template>
          <span v-else class="text-3xl font-semibold tabular-nums">—</span>
          <RouterLink :to="`/d/${d.day}`" class="link ml-auto text-xs">open</RouterLink>
        </div>
        <div class="mt-2 text-xs text-muted tabular-nums">
          {{ d.totals.kcal }} eaten
          <template v-if="rooms[i]">
            · {{ rooms[i]!.against }} {{ rooms[i]!.kind === 'burn' ? 'burnt' : 'target' }}
          </template>
          <template v-if="d.totals.sport_kcal"> · {{ d.totals.sport_kcal }} sport</template>
          <template v-if="d.totals.protein_g"> · {{ d.totals.protein_g }} g protein</template>
        </div>
      </div>

      <div class="card p-4" data-testid="tile-weight">
        <div class="text-xs tracking-wide text-muted uppercase">Weight</div>
        <div class="mt-1 flex items-baseline gap-2">
          <span class="text-3xl font-semibold tabular-nums">{{
            last ? formatKg(last.weight_g) : '—'
          }}</span>
          <span v-if="last" class="text-sm text-muted">kg</span>
          <RouterLink :to="`/d/${today}`" class="link ml-auto text-xs">weigh in</RouterLink>
        </div>
        <div class="mt-2 text-xs text-muted tabular-nums">
          <template v-if="last"
            >{{ weighedAgo
            }}<template v-if="last.trend_g">
              · trend {{ formatKg(last.trend_g) }}</template
            ></template
          >
          <template v-else>No weigh-in in the last 30 days.</template>
        </div>
      </div>
    </div>

    <div v-if="days.length" class="grid gap-4 md:grid-cols-2">
      <section
        v-for="(d, i) in days"
        :key="d.day"
        class="card p-4"
        :data-testid="i === 0 ? 'log-today' : 'log-yesterday'"
      >
        <div class="flex items-baseline gap-2">
          <h2 class="text-sm font-medium tracking-wide text-muted uppercase">
            {{ i === 0 ? 'Today' : 'Yesterday' }}
          </h2>
          <span class="text-xs text-muted">{{ dayLabel(d.day) }}</span>
          <span class="flex-1"></span>
          <span class="text-sm font-medium tabular-nums">{{ d.totals.kcal }}</span>
        </div>
        <table v-if="d.meals.length" class="mt-2 w-full text-sm">
          <tbody>
            <tr v-for="m in d.meals" :key="m.id" class="border-t border-edge">
              <td class="w-12 py-1 pr-3 font-mono text-xs text-muted">{{ time(m.eaten_at) }}</td>
              <td class="py-1">{{ m.description }}</td>
              <td class="py-1 text-right tabular-nums">{{ m.kcal }}</td>
            </tr>
          </tbody>
        </table>
        <p v-else class="mt-2 text-sm text-muted">Nothing logged.</p>
        <ul v-if="d.activities.length" class="mt-2 border-t border-edge pt-2 text-sm">
          <li v-for="a in d.activities" :key="a.id" class="flex gap-2 py-0.5">
            <span class="font-mono text-xs text-muted">{{ time(a.started_at) }}</span>
            <span>{{ a.kind }}</span>
            <span class="text-muted">{{ formatMinutes(a.minutes) }}</span>
            <span class="flex-1"></span>
            <span v-if="a.kcal" class="tabular-nums text-muted">{{ a.kcal }} kcal</span>
          </li>
        </ul>
      </section>
    </div>

    <BudgetChart
      v-if="week && week.logged_days > 0"
      :week="week"
      :today="today"
      data-testid="home-chart"
    />
  </main>
</template>
