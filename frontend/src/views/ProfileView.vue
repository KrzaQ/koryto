<script setup lang="ts">
// A person's settings: the profile behind the expenditure seed, targets
// and the location history. The person chooser in the nav picks whose.
import { computed, onMounted, reactive, ref, watch } from 'vue'
import { ApiError, api } from '@/api/client'
import type { LocationDto, TargetDto } from '@/api/types'
import { formatDateTime, fromLocalInput, nowLocal, zoneAbbr } from '@/lib/time'
import { looksLikeKg } from '@/lib/units'
import { usePerson } from '@/stores/person'
import { useSession } from '@/stores/session'
import { useTimezone } from '@/stores/timezone'

const session = useSession()
const person = usePerson()
const tz = useTimezone()

const error = ref<string | null>(null)
const saved = ref<string | null>(null)
const targets = ref<TargetDto[]>([])
const locations = ref<LocationDto[]>([])

// The profile form only knows my own row in full; for another member the
// fields come back from the patch response.
const profile = reactive({
  name: '',
  day_boundary: '04:00',
  height_cm: '',
  born_on: '',
  sex: '',
  activity_factor: '1.40',
  loaded: false,
})
const isMe = computed(() => person.isMe)

function fillFromMe() {
  const u = session.me?.user
  if (!u || !isMe.value) {
    profile.loaded = false
    return
  }
  profile.name = u.name ?? ''
  profile.day_boundary = minutesToClock(u.day_boundary_minutes)
  profile.height_cm = u.height_mm == null ? '' : String(u.height_mm / 10)
  profile.born_on = u.born_on ?? ''
  profile.sex = u.sex ?? ''
  profile.activity_factor = u.activity_factor
  profile.loaded = true
}
function minutesToClock(m: number) {
  return `${String(Math.floor(m / 60)).padStart(2, '0')}:${String(m % 60).padStart(2, '0')}`
}
function clockToMinutes(s: string) {
  const [h, m] = s.split(':').map(Number)
  return (h ?? 0) * 60 + (m ?? 0)
}

async function load() {
  error.value = null
  try {
    ;[targets.value, locations.value] = await Promise.all([
      api.targets.list(person.id),
      api.locations.list(person.id),
    ])
  } catch (e) {
    error.value = e instanceof ApiError ? e.message : String(e)
  }
  fillFromMe()
}

async function run(f: () => Promise<unknown>, note = 'Saved') {
  error.value = null
  saved.value = null
  try {
    await f()
    saved.value = note
    await load()
    return true
  } catch (e) {
    error.value = e instanceof ApiError ? e.message : String(e)
    return false
  }
}

async function saveProfile() {
  const ok = await run(() =>
    api.profile.update(person.id, {
      name: profile.name.trim() || undefined,
      day_boundary_minutes: clockToMinutes(profile.day_boundary),
      height_mm:
        profile.height_cm.trim() === ''
          ? null
          : Math.round(Number(profile.height_cm.replace(',', '.')) * 10),
      born_on: profile.born_on || null,
      sex: profile.sex || null,
      activity_factor: profile.activity_factor.trim(),
    }),
  )
  if (ok && isMe.value) await session.load()
  fillFromMe()
}

const targetForm = reactive({ open: false, valid_from: '', kcal: '', protein_g: '', weight_kg: '' })
function openTarget() {
  targetForm.open = true
  targetForm.valid_from = session.me?.today ?? ''
}
const targetValid = computed(
  () =>
    /^\d+$/.test(targetForm.kcal.trim()) &&
    Number(targetForm.kcal) > 0 &&
    (targetForm.protein_g.trim() === '' || /^\d+$/.test(targetForm.protein_g.trim())) &&
    (targetForm.weight_kg.trim() === '' || looksLikeKg(targetForm.weight_kg)),
)
async function addTarget() {
  if (!targetValid.value) return
  const ok = await run(() =>
    api.targets.create(person.id, {
      valid_from: targetForm.valid_from || null,
      kcal: Number(targetForm.kcal),
      protein_g: targetForm.protein_g.trim() ? Number(targetForm.protein_g) : null,
      weight_kg: targetForm.weight_kg.trim() || null,
    }),
  )
  if (ok) {
    Object.assign(targetForm, { open: false, kcal: '', protein_g: '', weight_kg: '' })
    if (isMe.value) await session.load()
  }
}
function removeTarget(t: TargetDto) {
  if (!window.confirm(`Remove the target from ${t.valid_from}?`)) return
  run(() => api.targets.remove(person.id, t.id), 'Target removed')
}

const locationForm = reactive({ open: false, timezone: '', from: '' })
function openLocation() {
  locationForm.open = true
  locationForm.from = nowLocal(tz.zone)
}
async function addLocation() {
  if (!locationForm.timezone.trim()) return
  const ok = await run(() =>
    api.locations.create(person.id, {
      timezone: locationForm.timezone.trim(),
      valid_from: fromLocalInput(locationForm.from, tz.zone),
    }),
  )
  if (ok) {
    Object.assign(locationForm, { open: false, timezone: '' })
    if (isMe.value) await session.load()
  }
}
async function removeLocation(l: LocationDto) {
  if (
    !window.confirm(
      `Remove ${l.timezone} from ${formatDateTime(l.valid_from, tz.zone)}? Days recompute.`,
    )
  )
    return
  if ((await run(() => api.locations.remove(person.id, l.id), 'Location removed')) && isMe.value)
    await session.load()
}
async function changeOriginZone(l: LocationDto) {
  const zone = window.prompt('Zone for the origin row (IANA name):', l.timezone)
  if (!zone || zone === l.timezone) return
  if ((await run(() => api.locations.update(person.id, l.id, { timezone: zone }))) && isMe.value)
    await session.load()
}

watch(() => person.id, load)
onMounted(load)
</script>

<template>
  <main class="mx-auto max-w-4xl space-y-6 px-4 py-6">
    <div class="flex items-center gap-3">
      <h1 class="text-xl font-semibold">Profile</h1>
      <span class="chip" data-testid="profile-person">{{ person.name }}</span>
      <span class="flex-1"></span>
      <span v-if="saved" class="text-sm text-ok">{{ saved }}</span>
    </div>
    <p v-if="error" class="note-danger">{{ error }}</p>

    <section class="card p-4" data-testid="profile-form">
      <h2 class="text-sm font-medium tracking-wide text-muted uppercase">Settings</h2>
      <p class="mt-1 text-xs text-muted">
        Height, birth date and sex only feed the Mifflin-St Jeor seed shown until there is enough
        data for the adaptive expenditure estimate. The day boundary is when a new day starts.
      </p>
      <p v-if="!profile.loaded" class="mt-3 text-sm text-muted">
        Another member's settings are edited from their own login.
      </p>
      <form v-else class="mt-3 grid gap-3 sm:grid-cols-3" @submit.prevent="saveProfile">
        <label class="text-sm">Name<br /><input v-model="profile.name" class="w-full" /></label>
        <label class="text-sm"
          >Day starts at<br /><input v-model="profile.day_boundary" type="time" class="w-full"
        /></label>
        <label class="text-sm"
          >Activity factor<br /><input
            v-model="profile.activity_factor"
            class="w-full"
            placeholder="1.40"
        /></label>
        <label class="text-sm"
          >Height (cm)<br /><input v-model="profile.height_cm" class="w-full" inputmode="decimal"
        /></label>
        <label class="text-sm"
          >Born on<br /><input v-model="profile.born_on" type="date" class="w-full"
        /></label>
        <label class="text-sm"
          >Sex<br />
          <select v-model="profile.sex" class="w-full">
            <option value="">—</option>
            <option value="female">female</option>
            <option value="male">male</option>
          </select>
        </label>
        <div class="sm:col-span-3">
          <button class="btn" type="submit" data-testid="profile-save">Save</button>
        </div>
      </form>
    </section>

    <section class="card">
      <div class="flex items-center gap-2 px-4 py-3">
        <h2 class="text-sm font-medium tracking-wide text-muted uppercase">Targets</h2>
        <span class="flex-1"></span>
        <button
          v-if="!targetForm.open"
          class="link text-xs"
          data-testid="add-target"
          @click="openTarget"
        >
          + Add
        </button>
      </div>
      <table class="w-full text-sm">
        <thead class="table-head">
          <tr>
            <th class="px-3 py-2">From</th>
            <th class="px-3 py-2 text-right">kcal</th>
            <th class="px-3 py-2 text-right">Protein</th>
            <th class="px-3 py-2 text-right">Goal weight</th>
            <th></th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="t in targets"
            :key="t.id"
            class="border-t border-edge"
            data-testid="target-row"
          >
            <td class="px-3 py-2 font-mono text-xs">{{ t.valid_from }}</td>
            <td class="px-3 py-2 text-right tabular-nums">{{ t.kcal }}</td>
            <td class="px-3 py-2 text-right tabular-nums">{{ t.protein_g ?? '—' }}</td>
            <td class="px-3 py-2 text-right tabular-nums">
              {{ t.weight_kg ? `${t.weight_kg} kg` : '—' }}
            </td>
            <td class="px-3 py-2 text-right">
              <button class="link text-xs text-danger" @click="removeTarget(t)">remove</button>
            </td>
          </tr>
          <tr
            v-if="targetForm.open"
            class="border-t border-edge bg-accent-soft"
            data-testid="target-editor"
          >
            <td class="px-2 py-1">
              <input v-model="targetForm.valid_from" type="date" class="font-mono text-xs" />
            </td>
            <td class="px-2 py-1">
              <input
                v-model="targetForm.kcal"
                class="w-20 text-right"
                inputmode="numeric"
                placeholder="kcal"
                data-testid="target-kcal"
              />
            </td>
            <td class="px-2 py-1">
              <input
                v-model="targetForm.protein_g"
                class="w-16 text-right"
                inputmode="numeric"
                placeholder="g"
              />
            </td>
            <td class="px-2 py-1">
              <input v-model="targetForm.weight_kg" class="w-20 text-right" placeholder="kg" />
            </td>
            <td class="px-2 py-1 text-right whitespace-nowrap">
              <button
                class="btn"
                :disabled="!targetValid"
                data-testid="target-save"
                @click="addTarget"
              >
                Add
              </button>
              <button class="btn-secondary ml-1" @click="targetForm.open = false">Cancel</button>
            </td>
          </tr>
          <tr v-if="!targets.length && !targetForm.open">
            <td colspan="5" class="px-3 py-4 text-center text-muted">No target yet.</td>
          </tr>
        </tbody>
      </table>
    </section>

    <section class="card">
      <div class="flex items-center gap-2 px-4 py-3">
        <h2 class="text-sm font-medium tracking-wide text-muted uppercase">Locations</h2>
        <span class="flex-1"></span>
        <button
          v-if="!locationForm.open"
          class="link text-xs"
          data-testid="add-location"
          @click="openLocation"
        >
          + Add
        </button>
      </div>
      <p class="px-4 pb-2 text-xs text-muted">
        Where the person was from when: the clock every entry's day is computed on. The origin row
        covers everything before the first move.
      </p>
      <table class="w-full text-sm">
        <tbody>
          <tr
            v-for="l in locations"
            :key="l.id"
            class="border-t border-edge"
            data-testid="location-row"
          >
            <td class="px-3 py-2 font-mono text-xs">
              {{ l.origin ? 'origin' : formatDateTime(l.valid_from, tz.zone) }}
            </td>
            <td class="px-3 py-2">
              {{ l.timezone }} <span class="text-xs text-muted">{{ zoneAbbr(l.timezone) }}</span>
            </td>
            <td class="px-3 py-2 text-right">
              <button v-if="l.origin" class="link text-xs" @click="changeOriginZone(l)">
                change zone
              </button>
              <button v-else class="link text-xs text-danger" @click="removeLocation(l)">
                remove
              </button>
            </td>
          </tr>
          <tr
            v-if="locationForm.open"
            class="border-t border-edge bg-accent-soft"
            data-testid="location-editor"
          >
            <td class="px-2 py-1">
              <input v-model="locationForm.from" type="datetime-local" class="font-mono text-xs" />
            </td>
            <td class="px-2 py-1">
              <input
                v-model="locationForm.timezone"
                class="w-56"
                placeholder="America/New_York"
                data-testid="location-zone"
              />
            </td>
            <td class="px-2 py-1 text-right whitespace-nowrap">
              <button
                class="btn"
                :disabled="!locationForm.timezone.trim()"
                data-testid="location-save"
                @click="addLocation"
              >
                Add
              </button>
              <button class="btn-secondary ml-1" @click="locationForm.open = false">Cancel</button>
            </td>
          </tr>
        </tbody>
      </table>
    </section>
  </main>
</template>
