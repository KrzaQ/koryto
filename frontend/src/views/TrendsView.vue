<script setup lang="ts">
// One person's story over a range: weight and its trend, intake against the
// target, the expenditure estimate, weekly balance, and how faithfully the
// log was kept. Every chart has one axis; colours sit in fixed palette
// slots so the same thing is the same colour everywhere on the page.
import { computed, onMounted, ref, watch } from 'vue'
import { use } from 'echarts/core'
import { BarChart, HeatmapChart, LineChart, ScatterChart } from 'echarts/charts'
import {
  CalendarComponent,
  GridComponent,
  LegendComponent,
  MarkLineComponent,
  TooltipComponent,
  VisualMapComponent,
} from 'echarts/components'
import { CanvasRenderer } from 'echarts/renderers'
import VChart from 'vue-echarts'
import { ApiError, api } from '@/api/client'
import type { DayRow, ExpenditureStats, WeeklyStats, WeightStats } from '@/api/types'
import { shiftDay, shortDayLabel } from '@/lib/day'
import { OTHER, chartInk, seriesColor } from '@/lib/palette'
import { formatKg, formatMinutes, signed } from '@/lib/units'
import { usePerson } from '@/stores/person'
import { useSession } from '@/stores/session'
import { useTheme } from '@/stores/theme'

use([
  BarChart,
  HeatmapChart,
  LineChart,
  ScatterChart,
  CalendarComponent,
  GridComponent,
  LegendComponent,
  MarkLineComponent,
  TooltipComponent,
  VisualMapComponent,
  CanvasRenderer,
])

// Palette slots, fixed for the page: 0 weight, 1 intake, 2 expenditure, 3 sport.
const SLOT = { weight: 0, intake: 1, expenditure: 2, sport: 3 } as const

const session = useSession()
const person = usePerson()
const theme = useTheme()
const ink = computed(() => chartInk(theme.resolved))

const to = ref(session.me?.today ?? new Date().toISOString().slice(0, 10))
const weeks = ref(13)
const from = computed(() => shiftDay(to.value, -(weeks.value * 7 - 1)))
const balanceMode = ref<'expenditure' | 'target'>('expenditure')
const showTable = ref(false)

const days = ref<DayRow[]>([])
const weight = ref<WeightStats | null>(null)
const expenditure = ref<ExpenditureStats | null>(null)
const weekly = ref<WeeklyStats | null>(null)
const error = ref<string | null>(null)

async function load() {
  error.value = null
  const r = { user: person.id, from: from.value, to: to.value }
  try {
    const [d, w, e, k] = await Promise.all([
      api.days(r),
      api.stats.weight(r),
      api.stats.expenditure(r),
      api.stats.weekly(r),
    ])
    days.value = d.days
    weight.value = w
    expenditure.value = e
    weekly.value = k
  } catch (e) {
    error.value = e instanceof ApiError ? e.message : String(e)
  }
}

// --- shared chart chrome -----------------------------------------------------

const axisStyle = computed(() => ({
  axisLine: { show: false },
  axisTick: { show: false },
  axisLabel: { color: ink.value.muted },
  splitLine: { lineStyle: { color: ink.value.grid } },
}))
const base = computed(() => ({
  textStyle: { color: ink.value.text, fontFamily: 'inherit' },
  grid: { left: 56, right: 16, top: 32, bottom: 32 },
  legend: { top: 0, left: 0, icon: 'roundRect', textStyle: { color: ink.value.muted } },
}))
const timeAxis = computed(() => ({
  type: 'time' as const,
  min: `${from.value}T00:00:00Z`,
  max: `${to.value}T00:00:00Z`,
  ...axisStyle.value,
  splitLine: { show: false },
}))
const dayOf = (d: string) => `${d}T00:00:00Z`
const kg = (g: number) => `${formatKg(g)} kg`

/** Trailing 7-day mean of intake over logged days; null where fewer than 3. */
const intake7 = computed(() =>
  days.value.map((_, i) => {
    const window = days.value.slice(Math.max(0, i - 6), i + 1).filter((d) => d.logged)
    if (window.length < 3) return null
    return Math.round(window.reduce((a, d) => a + (d.kcal ?? 0), 0) / window.length)
  }),
)

// --- 1. weight -----------------------------------------------------------------

const weightOption = computed(() => {
  const pts = weight.value?.points ?? []
  const goal = weight.value?.goal_g
  const all = pts.flatMap((p) => [p.weight_g, p.trend_g]).concat(goal ? [goal] : [])
  const pad = 500
  return {
    ...base.value,
    color: [seriesColor(SLOT.weight, theme.resolved), seriesColor(SLOT.weight, theme.resolved)],
    tooltip: {
      trigger: 'axis',
      valueFormatter: (v: number) => kg(v),
    },
    xAxis: timeAxis.value,
    yAxis: {
      type: 'value',
      name: 'kg',
      nameTextStyle: { color: ink.value.muted, align: 'right' },
      min: all.length ? Math.floor((Math.min(...all) - pad) / 1000) * 1000 : undefined,
      max: all.length ? Math.ceil((Math.max(...all) + pad) / 1000) * 1000 : undefined,
      ...axisStyle.value,
      axisLabel: { color: ink.value.muted, formatter: (v: number) => formatKg(v) },
    },
    series: [
      {
        name: 'Reading',
        type: 'scatter',
        symbolSize: 8,
        itemStyle: { opacity: 0.45, borderColor: ink.value.surface, borderWidth: 2 },
        data: pts.map((p) => [dayOf(p.day), p.weight_g]),
      },
      {
        name: 'Trend',
        type: 'line',
        showSymbol: false,
        lineStyle: { width: 2 },
        data: pts.map((p) => [dayOf(p.day), p.trend_g]),
        markLine: goal
          ? {
              silent: true,
              symbol: 'none',
              lineStyle: { color: OTHER, type: 'solid', width: 1 },
              label: { color: ink.value.muted, formatter: `goal ${formatKg(goal)}` },
              data: [{ yAxis: goal }],
            }
          : undefined,
      },
    ],
  }
})

// --- 2. intake ------------------------------------------------------------------

const intakeOption = computed(() => ({
  ...base.value,
  color: [
    seriesColor(SLOT.intake, theme.resolved),
    seriesColor(SLOT.intake, theme.resolved),
    OTHER,
  ],
  tooltip: {
    trigger: 'axis',
    valueFormatter: (v: number | null) => (v === null ? 'not logged' : `${v} kcal`),
  },
  xAxis: {
    type: 'category',
    data: days.value.map((d) => d.day),
    ...axisStyle.value,
    splitLine: { show: false },
    axisLabel: { color: ink.value.muted, formatter: (d: string) => shortDayLabel(d) },
  },
  yAxis: {
    type: 'value',
    name: 'kcal',
    nameTextStyle: { color: ink.value.muted, align: 'right' },
    ...axisStyle.value,
  },
  series: [
    {
      name: 'Intake',
      type: 'bar',
      barMaxWidth: 14,
      itemStyle: { opacity: 0.45, borderRadius: [3, 3, 0, 0] },
      data: days.value.map((d) => (d.logged ? d.kcal : null)),
    },
    {
      name: '7-day mean',
      type: 'line',
      showSymbol: false,
      connectNulls: false,
      lineStyle: { width: 2 },
      data: intake7.value,
    },
    {
      name: 'Target',
      type: 'line',
      showSymbol: false,
      step: 'end',
      lineStyle: { width: 1 },
      data: days.value.map((d) => d.target_kcal ?? null),
    },
  ],
}))

// --- 3. expenditure -------------------------------------------------------------

const expenditureOption = computed(() => {
  const pts = expenditure.value?.days ?? []
  const adaptive = pts.map((p) => (p.basis === 'adaptive' ? p.kcal : null))
  const seed = pts.map((p) => (p.basis === 'seed' ? p.kcal : null))
  return {
    ...base.value,
    color: [
      seriesColor(SLOT.expenditure, theme.resolved),
      seriesColor(SLOT.expenditure, theme.resolved),
      seriesColor(SLOT.intake, theme.resolved),
    ],
    tooltip: {
      trigger: 'axis',
      valueFormatter: (v: number | null) => (v === null ? '—' : `${v} kcal`),
    },
    xAxis: {
      type: 'category',
      data: pts.map((p) => p.day),
      ...axisStyle.value,
      splitLine: { show: false },
      axisLabel: { color: ink.value.muted, formatter: (d: string) => shortDayLabel(d) },
    },
    yAxis: {
      type: 'value',
      name: 'kcal/day',
      nameTextStyle: { color: ink.value.muted, align: 'right' },
      ...axisStyle.value,
    },
    series: [
      {
        name: 'Expenditure',
        type: 'line',
        showSymbol: false,
        lineStyle: { width: 2 },
        data: adaptive,
      },
      {
        name: 'Seed (Mifflin-St Jeor)',
        type: 'line',
        showSymbol: false,
        lineStyle: { width: 2, type: 'dashed' },
        data: seed,
      },
      {
        name: 'Intake, 7-day mean',
        type: 'line',
        showSymbol: false,
        lineStyle: { width: 2 },
        data: intake7.value,
      },
    ],
  }
})

// --- 4. weekly balance and sport ---------------------------------------------------

const weekRows = computed(() => weekly.value?.weeks ?? [])
const balanceOf = (w: {
  mean_balance_vs_expenditure?: number | null
  mean_balance_vs_target?: number | null
}) =>
  balanceMode.value === 'expenditure'
    ? (w.mean_balance_vs_expenditure ?? null)
    : (w.mean_balance_vs_target ?? null)

const balanceOption = computed(() => ({
  ...base.value,
  legend: undefined,
  tooltip: {
    trigger: 'axis',
    valueFormatter: (v: number | null) => (v === null ? 'no estimate' : `${signed(v)} kcal/day`),
  },
  xAxis: {
    type: 'category',
    data: weekRows.value.map((w) => w.week.slice(5)),
    ...axisStyle.value,
    splitLine: { show: false },
  },
  yAxis: {
    type: 'value',
    name: 'kcal/day',
    nameTextStyle: { color: ink.value.muted, align: 'right' },
    ...axisStyle.value,
  },
  series: [
    {
      name: balanceMode.value === 'expenditure' ? 'Intake − expenditure' : 'Intake − target',
      type: 'bar',
      barMaxWidth: 28,
      itemStyle: {
        borderRadius: 3,
        // Diverging: surplus warm, deficit cool, nothing at zero.
        color: (p: { value: number | null }) =>
          p.value === null
            ? OTHER
            : p.value > 0
              ? seriesColor(1, theme.resolved)
              : seriesColor(0, theme.resolved),
      },
      data: weekRows.value.map(balanceOf),
    },
  ],
}))

const sportOption = computed(() => ({
  ...base.value,
  legend: undefined,
  grid: { left: 56, right: 16, top: 8, bottom: 24 },
  color: [seriesColor(SLOT.sport, theme.resolved)],
  tooltip: { trigger: 'axis', valueFormatter: (v: number) => formatMinutes(v) },
  xAxis: {
    type: 'category',
    data: weekRows.value.map((w) => w.week.slice(5)),
    ...axisStyle.value,
    splitLine: { show: false },
  },
  yAxis: {
    type: 'value',
    name: 'min',
    nameTextStyle: { color: ink.value.muted, align: 'right' },
    ...axisStyle.value,
    axisLabel: { color: ink.value.muted, formatter: (v: number) => (v ? formatMinutes(v) : '0') },
  },
  series: [
    {
      name: 'Sport',
      type: 'bar',
      barMaxWidth: 28,
      itemStyle: { borderRadius: 3 },
      data: weekRows.value.map((w) => w.sport_minutes),
    },
  ],
}))

// --- 5. logging calendar -----------------------------------------------------------

const calendarOption = computed(() => {
  const max = Math.max(1, ...days.value.map((d) => d.meals))
  return {
    textStyle: { color: ink.value.text, fontFamily: 'inherit' },
    tooltip: {
      formatter: (p: { value: [string, number] }) =>
        `${shortDayLabel(p.value[0])}: ${p.value[1]} meal${p.value[1] === 1 ? '' : 's'}`,
    },
    visualMap: {
      min: 0,
      max,
      show: false,
      // One hue, light to dark: the weight slot, since this is not a series.
      inRange: { color: [ink.value.grid, seriesColor(SLOT.weight, theme.resolved)] },
    },
    calendar: {
      top: 24,
      left: 40,
      right: 16,
      range: [from.value, to.value],
      cellSize: ['auto', 14],
      splitLine: { show: false },
      itemStyle: { borderWidth: 2, borderColor: ink.value.surface, color: ink.value.surface },
      dayLabel: {
        color: ink.value.muted,
        firstDay: 1,
        nameMap: ['S', 'M', 'T', 'W', 'T', 'F', 'S'],
      },
      monthLabel: { color: ink.value.muted },
      yearLabel: { show: false },
    },
    series: [
      {
        type: 'heatmap',
        coordinateSystem: 'calendar',
        data: days.value.map((d) => [d.day, d.meals]),
      },
    ],
  }
})

const latest = computed(() => expenditure.value?.latest ?? null)
const loggedDays = computed(() => days.value.filter((d) => d.logged).length)

watch([from, to, () => person.id], load)
onMounted(load)
</script>

<template>
  <main class="mx-auto max-w-6xl space-y-6 px-4 py-6">
    <div class="flex flex-wrap items-center gap-4">
      <h1 class="text-2xl font-semibold">Trends</h1>
      <span v-if="!person.isMe" class="chip">{{ person.name }}</span>
      <span class="flex-1"></span>
      <label class="text-sm text-muted"
        >Until <input v-model="to" type="date" class="ml-1 font-mono text-xs"
      /></label>
      <label class="text-sm text-muted"
        >Weeks
        <select v-model.number="weeks" class="ml-1">
          <option :value="4">4</option>
          <option :value="8">8</option>
          <option :value="13">13</option>
          <option :value="26">26</option>
          <option :value="52">52</option>
        </select>
      </label>
      <label class="flex items-center gap-1 text-sm text-muted"
        ><input v-model="showTable" type="checkbox" /> table</label
      >
    </div>
    <p v-if="error" class="note-danger">{{ error }}</p>

    <div class="grid gap-4 md:grid-cols-3" data-testid="trend-tiles">
      <div class="card p-4">
        <div class="text-xs tracking-wide text-muted uppercase">Expenditure now</div>
        <div class="mt-1 flex items-baseline gap-2">
          <span class="text-2xl font-semibold tabular-nums">{{ latest?.kcal ?? '—' }}</span>
          <span class="text-sm text-muted">kcal/day</span>
        </div>
        <div class="mt-1 text-xs text-muted">
          <template v-if="latest?.basis === 'adaptive'"
            >From {{ latest.logged_days }} logged days and the weight trend<span
              v-if="latest.seed_kcal"
            >
              (seed would say {{ latest.seed_kcal }})</span
            >.</template
          >
          <template v-else-if="latest?.basis === 'seed'"
            >Mifflin-St Jeor seed; {{ latest.logged_days }} of 14 logged days and
            {{ latest.weight_span_days }} of 10 weigh-in days so far.</template
          >
          <template v-else>Needs weigh-ins and a profile (height, birth date, sex).</template>
        </div>
      </div>
      <div class="card p-4">
        <div class="text-xs tracking-wide text-muted uppercase">Trend weight</div>
        <div class="mt-1 flex items-baseline gap-2">
          <span class="text-2xl font-semibold tabular-nums">{{
            weight?.points.length ? formatKg(weight.points[weight.points.length - 1]!.trend_g) : '—'
          }}</span>
          <span class="text-sm text-muted">kg</span>
          <span
            v-if="weight && weight.points.length > 1"
            class="ml-auto text-sm tabular-nums text-muted"
            >{{
              signed(weight.points[weight.points.length - 1]!.trend_g - weight.points[0]!.trend_g)
            }}
            g</span
          >
        </div>
        <div class="mt-1 text-xs text-muted">
          Over the range<span v-if="weight?.goal_g">; goal {{ formatKg(weight.goal_g) }} kg</span>.
        </div>
      </div>
      <div class="card p-4">
        <div class="text-xs tracking-wide text-muted uppercase">Logged</div>
        <div class="mt-1 flex items-baseline gap-2">
          <span class="text-2xl font-semibold tabular-nums">{{ loggedDays }}</span>
          <span class="text-sm text-muted">of {{ days.length }} days</span>
        </div>
        <div class="mt-1 text-xs text-muted">Unlogged days are gaps, never zeros.</div>
      </div>
    </div>

    <section class="card p-4">
      <h2 class="text-sm font-medium tracking-wide text-muted uppercase">Weight</h2>
      <p class="text-xs text-muted">Each reading, and the trend that smooths them.</p>
      <div v-if="weight" class="h-64 w-full" data-testid="weight-chart">
        <VChart :option="weightOption" autoresize />
      </div>
    </section>

    <section class="card p-4">
      <h2 class="text-sm font-medium tracking-wide text-muted uppercase">Intake</h2>
      <p class="text-xs text-muted">
        Each day's kcal against the target, with the 7-day mean over logged days.
      </p>
      <div v-if="days.length" class="h-64 w-full" data-testid="intake-chart">
        <VChart :option="intakeOption" autoresize />
      </div>
    </section>

    <section class="card p-4">
      <h2 class="text-sm font-medium tracking-wide text-muted uppercase">Expenditure</h2>
      <p class="text-xs text-muted">
        What the body burns per day, derived from intake and the weight trend over a 28-day window.
        Dashed while the Mifflin-St Jeor seed stands in.
      </p>
      <div v-if="expenditure" class="h-64 w-full" data-testid="expenditure-chart">
        <VChart :option="expenditureOption" autoresize />
      </div>
    </section>

    <section class="card p-4">
      <div class="flex flex-wrap items-center gap-3">
        <h2 class="text-sm font-medium tracking-wide text-muted uppercase">Weekly balance</h2>
        <span class="flex-1"></span>
        <div
          class="inline-flex overflow-hidden rounded border border-edge text-xs"
          role="radiogroup"
          aria-label="Balance against"
          data-testid="balance-mode"
        >
          <button
            type="button"
            role="radio"
            :aria-checked="balanceMode === 'expenditure'"
            class="px-2 py-1"
            :class="
              balanceMode === 'expenditure'
                ? 'bg-accent text-white'
                : 'bg-surface text-muted hover:bg-surface-2'
            "
            @click="balanceMode = 'expenditure'"
          >
            vs expenditure
          </button>
          <button
            type="button"
            role="radio"
            :aria-checked="balanceMode === 'target'"
            class="px-2 py-1"
            :class="
              balanceMode === 'target'
                ? 'bg-accent text-white'
                : 'bg-surface text-muted hover:bg-surface-2'
            "
            @click="balanceMode = 'target'"
          >
            vs target
          </button>
        </div>
      </div>
      <p class="text-xs text-muted">
        Mean daily intake minus what was burnt, or minus the target. Below zero is a deficit.
      </p>
      <div v-if="weekly" class="h-56 w-full" data-testid="balance-chart">
        <VChart :option="balanceOption" autoresize />
      </div>
      <h3 class="mt-4 text-xs font-medium tracking-wide text-muted uppercase">Sport per week</h3>
      <div v-if="weekly" class="h-28 w-full" data-testid="sport-chart">
        <VChart :option="sportOption" autoresize />
      </div>
    </section>

    <section class="card p-4">
      <h2 class="text-sm font-medium tracking-wide text-muted uppercase">Logging</h2>
      <p class="text-xs text-muted">
        Meals logged per day; a blank cell is a day nothing was written down.
      </p>
      <div
        v-if="days.length"
        class="w-full"
        :style="{ height: `${40 + 7 * 16 + 16}px` }"
        data-testid="calendar-chart"
      >
        <VChart :option="calendarOption" autoresize />
      </div>
    </section>

    <section v-if="showTable" class="card overflow-x-auto" data-testid="trends-table">
      <table class="w-full text-sm">
        <thead class="table-head">
          <tr>
            <th class="px-3 py-2">Week</th>
            <th class="px-3 py-2 text-right">Logged</th>
            <th class="px-3 py-2 text-right">Mean kcal</th>
            <th class="px-3 py-2 text-right">Expenditure</th>
            <th class="px-3 py-2 text-right">vs expenditure</th>
            <th class="px-3 py-2 text-right">vs target</th>
            <th class="px-3 py-2 text-right">Sport</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="w in weekRows" :key="w.week" class="border-t border-edge">
            <td class="px-3 py-2 font-mono text-xs">{{ w.week }}</td>
            <td class="px-3 py-2 text-right tabular-nums">{{ w.logged_days }}/{{ w.days }}</td>
            <td class="px-3 py-2 text-right tabular-nums">{{ w.mean_kcal ?? '—' }}</td>
            <td class="px-3 py-2 text-right tabular-nums">{{ w.mean_expenditure ?? '—' }}</td>
            <td class="px-3 py-2 text-right tabular-nums">
              {{
                w.mean_balance_vs_expenditure == null ? '—' : signed(w.mean_balance_vs_expenditure)
              }}
            </td>
            <td class="px-3 py-2 text-right tabular-nums">
              {{ w.mean_balance_vs_target == null ? '—' : signed(w.mean_balance_vs_target) }}
            </td>
            <td class="px-3 py-2 text-right tabular-nums">
              {{ w.sport_minutes ? formatMinutes(w.sport_minutes) : '—' }}
            </td>
          </tr>
        </tbody>
      </table>
    </section>
  </main>
</template>
