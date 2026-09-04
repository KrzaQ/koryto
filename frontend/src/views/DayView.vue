<script setup lang="ts">
// One person's day: meals with inline edit and an add row, weigh-ins, sport.
import { computed, onMounted, reactive, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import { ApiError, api } from '@/api/client'
import type {
  ActivityDto,
  DayDto,
  MealInput,
  MealPatchInput,
  Summary,
  WeightDto,
} from '@/api/types'
import BudgetChart from '@/components/BudgetChart.vue'
import DayHeader from '@/components/DayHeader.vue'
import MealEditor from '@/components/MealEditor.vue'
import { dayLabel, shiftDay } from '@/lib/day'
import { formatDateTime, fromLocalInput, nowLocal, zoneCity } from '@/lib/time'
import { looksLikeDuration, looksLikeKg } from '@/lib/units'
import { usePerson } from '@/stores/person'
import { useSession } from '@/stores/session'
import { useTimezone } from '@/stores/timezone'

const props = defineProps<{ day: string }>()
const router = useRouter()
const session = useSession()
const person = usePerson()
const tz = useTimezone()

const data = ref<DayDto | null>(null)
// The six days before this one and this one: the week card and the chart.
const week = ref<Summary | null>(null)
const error = ref<string | null>(null)
const busy = ref(false)
const showVoided = ref(false)
const editing = ref<number | null>(null)
const adding = ref(false)

const members = computed(() =>
  session.members.map((m) => ({ id: m.id, name: m.name ?? m.email ?? `#${m.id}` })),
)
const isToday = computed(() => props.day === session.me?.today)
const zoneNote = computed(() => {
  const zones = new Set((data.value?.meals ?? []).map((m) => m.timezone))
  const mine = session.me?.timezone
  const other = [...zones].filter((z) => z !== mine)
  return other.length ? other.map(zoneCity).join(', ') : null
})

async function load() {
  error.value = null
  try {
    // The week is context: if it fails the day still shows.
    const [day, summary] = await Promise.all([
      api.day(person.id, props.day, showVoided.value),
      api.stats
        .summary({ user: person.id, from: shiftDay(props.day, -6), to: props.day })
        .catch(() => null),
    ])
    data.value = day
    week.value = summary
  } catch (e) {
    error.value = e instanceof ApiError ? e.message : String(e)
  }
}

async function run(f: () => Promise<unknown>) {
  busy.value = true
  error.value = null
  try {
    await f()
    await load()
    return true
  } catch (e) {
    error.value = e instanceof ApiError ? e.message : String(e)
    return false
  } finally {
    busy.value = false
  }
}

async function createMeal(input: MealInput) {
  if (await run(() => api.meals.create(input))) adding.value = false
}
async function saveMeal(id: number, patch: MealPatchInput) {
  if (await run(() => api.meals.update(id, patch))) editing.value = null
}
async function voidMeal(id: number) {
  if (await run(() => api.meals.void(id))) editing.value = null
}
function unvoid(kind: 'meals' | 'weights' | 'activities', id: number) {
  run(() => api[kind].unvoid(id))
}

// Weigh-ins and sport are small enough for one-line forms.
const weightForm = reactive({ open: false, kg: '', at: '' })
const sportForm = reactive({ open: false, kind: '', duration: '', kcal: '', at: '' })
function openWeight() {
  weightForm.open = true
  weightForm.at = nowLocal(tz.zone)
  if (!weightForm.at.startsWith(props.day)) weightForm.at = `${props.day}T07:00`
}
function openSport() {
  sportForm.open = true
  sportForm.at = nowLocal(tz.zone)
  if (!sportForm.at.startsWith(props.day)) sportForm.at = `${props.day}T17:00`
}
const weightValid = computed(() => looksLikeKg(weightForm.kg) && weightForm.at.length >= 16)
const sportValid = computed(
  () =>
    sportForm.kind.trim().length > 0 &&
    looksLikeDuration(sportForm.duration) &&
    sportForm.at.length >= 16,
)
async function addWeight() {
  if (!weightValid.value) return
  const ok = await run(() =>
    api.weights.create({
      user_id: person.id,
      weight_kg: weightForm.kg.trim(),
      measured_at: fromLocalInput(weightForm.at, tz.zone),
    }),
  )
  if (ok) Object.assign(weightForm, { open: false, kg: '' })
}
async function addSport() {
  if (!sportValid.value) return
  const ok = await run(() =>
    api.activities.create({
      user_id: person.id,
      kind: sportForm.kind.trim(),
      duration: sportForm.duration.trim(),
      kcal: sportForm.kcal.trim() ? Number(sportForm.kcal) : null,
      started_at: fromLocalInput(sportForm.at, tz.zone),
    }),
  )
  if (ok) Object.assign(sportForm, { open: false, kind: '', duration: '', kcal: '' })
}
function voidWeight(w: WeightDto) {
  run(() => api.weights.void(w.id))
}
function voidActivity(a: ActivityDto) {
  run(() => api.activities.void(a.id))
}

function go(day: string) {
  router.push(`/d/${day}`)
}
function time(iso: string) {
  return formatDateTime(iso, tz.zone).slice(11)
}
function rowClass(m: { voided: boolean }) {
  return m.voided ? 'text-faint line-through' : ''
}

watch(() => [props.day, person.id, showVoided.value], load)
onMounted(load)
</script>

<template>
  <main class="mx-auto max-w-6xl space-y-6 px-4 py-6">
    <div class="flex flex-wrap items-center gap-3">
      <button class="btn-secondary" title="Previous day" @click="go(shiftDay(day, -1))">‹</button>
      <h1 class="text-xl font-semibold" data-testid="day-title">
        {{ dayLabel(day) }}
        <span v-if="isToday" class="chip ml-1">today</span>
      </h1>
      <button class="btn-secondary" title="Next day" @click="go(shiftDay(day, 1))">›</button>
      <input
        type="date"
        :value="day"
        class="font-mono text-xs"
        @change="go(($event.target as HTMLInputElement).value)"
      />
      <span v-if="!person.isMe" class="chip" data-testid="person-note">{{ person.name }}</span>
      <span v-if="zoneNote" class="chip" :title="`Some entries were logged on another clock`"
        >on {{ zoneNote }} time</span
      >
      <span class="flex-1"></span>
      <label class="inline-flex items-center gap-1 text-xs text-muted">
        <input v-model="showVoided" type="checkbox" /> show voided
      </label>
    </div>

    <p v-if="error" class="note-danger" data-testid="error">{{ error }}</p>

    <template v-if="data">
      <DayHeader :day="data" :week="week" :is-today="isToday" />

      <BudgetChart
        v-if="week && week.logged_days > 0"
        :week="week"
        :today="session.me?.today ?? ''"
      />

      <section class="card overflow-x-auto">
        <table class="w-full text-sm">
          <thead class="table-head">
            <tr>
              <th class="px-3 py-2">Time</th>
              <th class="px-3 py-2">Meal</th>
              <th class="px-3 py-2 text-right">kcal</th>
              <th class="px-3 py-2 text-right">Protein</th>
              <th class="px-3 py-2">Source</th>
            </tr>
          </thead>
          <tbody>
            <template v-for="m in data.meals" :key="m.id">
              <MealEditor
                v-if="editing === m.id"
                :meal="m"
                :user-id="person.id"
                :busy="busy"
                @save="(p: MealPatchInput) => saveMeal(m.id, p)"
                @void="voidMeal(m.id)"
                @cancel="editing = null"
              />
              <tr
                v-else
                class="border-t border-edge"
                :class="rowClass(m)"
                data-testid="meal-row"
                :title="m.voided ? 'Voided' : 'Double-click to edit'"
                @dblclick="!m.voided && (editing = m.id)"
              >
                <td class="px-3 py-2 font-mono text-xs">{{ time(m.eaten_at) }}</td>
                <td class="px-3 py-2">
                  {{ m.description }}
                  <span v-if="m.portions && m.portions !== '1'" class="text-xs text-muted"
                    >× {{ m.portions }}</span
                  >
                  <span v-if="m.day_override" class="chip ml-1" title="Day set by hand"
                    >day set</span
                  >
                  <button v-if="m.voided" class="link ml-2 text-xs" @click="unvoid('meals', m.id)">
                    restore
                  </button>
                </td>
                <td class="px-3 py-2 text-right tabular-nums">{{ m.kcal }}</td>
                <td class="px-3 py-2 text-right tabular-nums">{{ m.protein_g ?? '—' }}</td>
                <td class="px-3 py-2">
                  <span class="chip">{{ m.source }}</span>
                </td>
              </tr>
            </template>
            <MealEditor
              v-if="adding"
              :day="day"
              :members="members"
              :user-id="person.id"
              :busy="busy"
              @create="createMeal"
              @cancel="adding = false"
            />
            <tr v-else class="border-t border-edge">
              <td colspan="5" class="px-3 py-2">
                <button class="link text-sm" data-testid="add-meal" @click="adding = true">
                  + Add a meal
                </button>
              </td>
            </tr>
          </tbody>
        </table>
      </section>

      <div class="grid gap-6 md:grid-cols-2">
        <section class="card">
          <div class="flex items-center gap-2 px-3 py-2">
            <h2 class="text-sm font-medium tracking-wide text-muted uppercase">Weigh-ins</h2>
            <span class="flex-1"></span>
            <button
              v-if="!weightForm.open"
              class="link text-xs"
              data-testid="add-weight"
              @click="openWeight"
            >
              + Add
            </button>
          </div>
          <table class="w-full text-sm">
            <tbody>
              <tr
                v-for="w in data.weights"
                :key="w.id"
                class="border-t border-edge"
                :class="rowClass(w)"
                data-testid="weight-row"
              >
                <td class="px-3 py-2 font-mono text-xs">{{ time(w.measured_at) }}</td>
                <td class="px-3 py-2 tabular-nums">{{ w.weight_kg }} kg</td>
                <td class="px-3 py-2 text-right">
                  <button v-if="!w.voided" class="link text-xs text-danger" @click="voidWeight(w)">
                    void
                  </button>
                  <button v-else class="link text-xs" @click="unvoid('weights', w.id)">
                    restore
                  </button>
                </td>
              </tr>
              <tr v-if="weightForm.open" class="border-t border-edge bg-accent-soft">
                <td class="px-2 py-1">
                  <input
                    v-model="weightForm.at"
                    type="datetime-local"
                    class="w-full font-mono text-xs"
                  />
                </td>
                <td class="px-2 py-1">
                  <input
                    v-model="weightForm.kg"
                    class="w-24 text-right"
                    placeholder="82.4"
                    data-testid="weight-input"
                    :class="{ 'border-red-500': weightForm.kg && !looksLikeKg(weightForm.kg) }"
                    @keydown.enter="addWeight"
                  />
                  kg
                </td>
                <td class="px-2 py-1 text-right whitespace-nowrap">
                  <button
                    class="btn"
                    :disabled="!weightValid || busy"
                    data-testid="weight-save"
                    @click="addWeight"
                  >
                    Add
                  </button>
                  <button class="btn-secondary ml-1" @click="weightForm.open = false">
                    Cancel
                  </button>
                </td>
              </tr>
              <tr v-if="!data.weights.length && !weightForm.open">
                <td colspan="3" class="px-3 py-4 text-center text-muted">No weigh-in.</td>
              </tr>
            </tbody>
          </table>
        </section>

        <section class="card">
          <div class="flex items-center gap-2 px-3 py-2">
            <h2 class="text-sm font-medium tracking-wide text-muted uppercase">Sport</h2>
            <span class="flex-1"></span>
            <button
              v-if="!sportForm.open"
              class="link text-xs"
              data-testid="add-sport"
              @click="openSport"
            >
              + Add
            </button>
          </div>
          <table class="w-full text-sm">
            <tbody>
              <tr
                v-for="a in data.activities"
                :key="a.id"
                class="border-t border-edge"
                :class="rowClass(a)"
                data-testid="activity-row"
              >
                <td class="px-3 py-2 font-mono text-xs">{{ time(a.started_at) }}</td>
                <td class="px-3 py-2">
                  {{ a.kind }} <span class="text-muted">{{ a.duration }}</span
                  ><span v-if="a.kcal" class="text-xs text-muted"> · {{ a.kcal }} kcal</span>
                </td>
                <td class="px-3 py-2 text-right">
                  <button
                    v-if="!a.voided"
                    class="link text-xs text-danger"
                    @click="voidActivity(a)"
                  >
                    void
                  </button>
                  <button v-else class="link text-xs" @click="unvoid('activities', a.id)">
                    restore
                  </button>
                </td>
              </tr>
              <tr v-if="sportForm.open" class="border-t border-edge bg-accent-soft">
                <td class="px-2 py-1">
                  <input
                    v-model="sportForm.at"
                    type="datetime-local"
                    class="w-full font-mono text-xs"
                  />
                </td>
                <td class="px-2 py-1">
                  <input
                    v-model="sportForm.kind"
                    class="w-24"
                    placeholder="run"
                    data-testid="sport-kind"
                  />
                  <input
                    v-model="sportForm.duration"
                    class="ml-1 w-16 text-right"
                    placeholder="45m"
                    data-testid="sport-duration"
                    :class="{
                      'border-red-500':
                        sportForm.duration && !looksLikeDuration(sportForm.duration),
                    }"
                  />
                  <input
                    v-model="sportForm.kcal"
                    class="ml-1 w-16 text-right"
                    placeholder="kcal"
                    inputmode="numeric"
                    @keydown.enter="addSport"
                  />
                </td>
                <td class="px-2 py-1 text-right whitespace-nowrap">
                  <button
                    class="btn"
                    :disabled="!sportValid || busy"
                    data-testid="sport-save"
                    @click="addSport"
                  >
                    Add
                  </button>
                  <button class="btn-secondary ml-1" @click="sportForm.open = false">Cancel</button>
                </td>
              </tr>
              <tr v-if="!data.activities.length && !sportForm.open">
                <td colspan="3" class="px-3 py-4 text-center text-muted">No sport.</td>
              </tr>
            </tbody>
          </table>
        </section>
      </div>
    </template>
  </main>
</template>
