<script setup lang="ts">
// The week as one column a day. The day's sport digs below the line, the
// intake stacks on top of it, and the base expenditure is the line to beat:
// a column ending below it is a day with room left, above it a day over.
// Same palette slots as the Trends page: 1 intake, 2 expenditure, 3 sport.
import { computed } from 'vue'
import { use } from 'echarts/core'
import { BarChart, LineChart } from 'echarts/charts'
import { GridComponent, LegendComponent, TooltipComponent } from 'echarts/components'
import { CanvasRenderer } from 'echarts/renderers'
import VChart from 'vue-echarts'
import type { Summary } from '@/api/types'
import { shiftDay, shortDayLabel } from '@/lib/day'
import { chartInk, seriesColor } from '@/lib/palette'
import { useTheme } from '@/stores/theme'

use([BarChart, LineChart, GridComponent, LegendComponent, TooltipComponent, CanvasRenderer])

const SLOT = { intake: 1, expenditure: 2, sport: 3 } as const

const props = defineProps<{ week: Summary; today: string }>()
const theme = useTheme()
const ink = computed(() => chartInk(theme.resolved))

const rows = computed(() => props.week.rows)
const estimates = computed(() => props.week.expenditure_days)
const hasBase = computed(() => estimates.value.some((e) => e.base_kcal != null))
const seed = computed(() => estimates.value[estimates.value.length - 1]?.basis === 'seed')

function label(day: string) {
  if (day === props.today) return 'Today'
  if (day === shiftDay(props.today, -1)) return 'Yesterday'
  return shortDayLabel(day)
}

/** eaten, sport, base and what the day left, for the tooltip. */
function detail(i: number) {
  const r = rows.value[i]
  const e = estimates.value[i]
  if (!r || !e) return ''
  if (!r.logged) return 'Nothing logged'
  const lines = [`${r.kcal} eaten`]
  if (e.sport_kcal) lines.push(`${e.sport_kcal} kcal of sport`)
  if (e.base_kcal != null) lines.push(`${e.base_kcal} base, ${e.kcal} burnt`)
  if (e.kcal != null && r.kcal != null) {
    const room = e.kcal - r.kcal
    lines.push(room < 0 ? `${-room} over` : `${room} left`)
  }
  return lines.join('<br>')
}

const option = computed(() => ({
  textStyle: { color: ink.value.text, fontFamily: 'inherit' },
  grid: { left: 56, right: 16, top: 32, bottom: 24 },
  legend: { top: 0, right: 0, icon: 'roundRect', textStyle: { color: ink.value.muted } },
  // In series order: sport, intake, base.
  color: [
    seriesColor(SLOT.sport, theme.resolved),
    seriesColor(SLOT.intake, theme.resolved),
    seriesColor(SLOT.expenditure, theme.resolved),
  ],
  tooltip: {
    trigger: 'axis',
    formatter: (ps: { dataIndex: number }[]) => {
      const i = ps[0]?.dataIndex ?? 0
      return `${shortDayLabel(rows.value[i]?.day ?? '')}<br>${detail(i)}`
    },
  },
  xAxis: {
    type: 'category',
    data: rows.value.map((r) => label(r.day)),
    axisLine: { show: false },
    axisTick: { show: false },
    splitLine: { show: false },
    axisLabel: { color: ink.value.muted },
  },
  yAxis: {
    type: 'value',
    name: 'kcal',
    nameTextStyle: { color: ink.value.muted, align: 'right' },
    axisLine: { show: false },
    axisTick: { show: false },
    axisLabel: { color: ink.value.muted },
    splitLine: { lineStyle: { color: ink.value.grid } },
  },
  series: [
    {
      name: 'Sport',
      type: 'bar',
      stack: 'day',
      barMaxWidth: 44,
      // The hole the session dug, below the line.
      data: estimates.value.map((e) => -e.sport_kcal),
    },
    {
      name: 'Intake',
      type: 'bar',
      stack: 'day',
      barMaxWidth: 44,
      // The food that is left once the sport is paid back, drawn from zero
      // up. With the sport below, the whole column is the day's intake and
      // splits at the line; stacking the full intake instead would paint
      // over the sport segment.
      data: rows.value.map((r, i) =>
        r.kcal == null ? null : r.kcal - (estimates.value[i]?.sport_kcal ?? 0),
      ),
    },
    {
      name: seed.value ? 'Base (seed)' : 'Base expenditure',
      type: 'line',
      step: 'middle',
      showSymbol: false,
      lineStyle: { width: 2, type: seed.value ? 'dashed' : 'solid' },
      data: estimates.value.map((e) => e.base_kcal),
    },
  ],
}))
</script>

<template>
  <section class="card p-4" data-testid="budget-chart">
    <h2 class="text-sm font-medium tracking-wide text-muted uppercase">The week</h2>
    <p class="text-xs text-muted">
      Each day's sport digs below the line and the food stacks on top of it, so the column is the
      day's intake: ending under the base line means there was room left.
    </p>
    <div class="mt-2 h-56 w-full"><VChart :option="option" autoresize /></div>
    <p v-if="!hasBase" class="note-warn mt-2" data-testid="budget-chart-note">
      No base line yet: it needs a weigh-in and height, birth date and sex on the
      <RouterLink to="/profile" class="link">profile</RouterLink>.
    </p>
  </section>
</template>
