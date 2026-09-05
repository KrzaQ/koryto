<script setup lang="ts">
// The reference data: the household's saved foods, and the MET rates that
// turn a duration of sport into kcal.
import { onMounted, reactive, ref, watch } from 'vue'
import { ApiError, api } from '@/api/client'
import type { FoodDto } from '@/api/types'
import ActivityKindsPanel from '@/components/ActivityKindsPanel.vue'

const tab = ref<'foods' | 'sport'>('foods')

const foods = ref<FoodDto[]>([])
const q = ref('')
const includeArchived = ref(false)
const error = ref<string | null>(null)
const busy = ref(false)
const editing = ref<number | null>(null)
const adding = ref(false)
const form = reactive({ name: '', aliases: '', portion: '', kcal: '', protein_g: '' })

async function load() {
  error.value = null
  try {
    foods.value = await api.foods.list(q.value.trim(), includeArchived.value)
  } catch (e) {
    error.value = e instanceof ApiError ? e.message : String(e)
  }
}

function startAdd() {
  Object.assign(form, { name: '', aliases: '', portion: '', kcal: '', protein_g: '' })
  editing.value = null
  adding.value = true
}
function startEdit(f: FoodDto) {
  Object.assign(form, {
    name: f.name,
    aliases: f.aliases.join(', '),
    portion: f.portion,
    kcal: String(f.kcal),
    protein_g: f.protein_g == null ? '' : String(f.protein_g),
  })
  adding.value = false
  editing.value = f.id
}
function valid() {
  return (
    form.name.trim().length > 0 &&
    form.portion.trim().length > 0 &&
    /^\d+$/.test(form.kcal.trim()) &&
    (form.protein_g.trim() === '' || /^\d+$/.test(form.protein_g.trim()))
  )
}
function payload() {
  return {
    name: form.name.trim(),
    aliases: form.aliases
      .split(',')
      .map((a) => a.trim())
      .filter(Boolean),
    portion: form.portion.trim(),
    kcal: Number(form.kcal),
    protein_g: form.protein_g.trim() === '' ? null : Number(form.protein_g),
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
async function save() {
  if (!valid()) return
  const ok =
    editing.value === null
      ? await run(() => api.foods.create(payload()))
      : await run(() => api.foods.update(editing.value!, payload()))
  if (ok) {
    adding.value = false
    editing.value = null
  }
}
function archive(f: FoodDto) {
  run(() => (f.archived_at ? api.foods.unarchive(f.id) : api.foods.archive(f.id)))
}

let timer: ReturnType<typeof setTimeout> | undefined
watch([q, includeArchived], () => {
  clearTimeout(timer)
  timer = setTimeout(load, 150)
})
onMounted(load)
</script>

<template>
  <main class="mx-auto max-w-5xl space-y-4 px-4 py-6">
    <div class="flex flex-wrap items-center gap-3">
      <h1 class="text-xl font-semibold">Reference</h1>
      <div class="flex rounded border border-edge" role="tablist">
        <button
          v-for="t in ['foods', 'sport'] as const"
          :key="t"
          class="px-3 py-1 text-sm capitalize"
          :class="tab === t ? 'bg-accent text-white' : 'text-muted hover:text-fg'"
          role="tab"
          :aria-selected="tab === t"
          :data-testid="`tab-${t}`"
          @click="tab = t"
        >
          {{ t === 'foods' ? 'Foods' : 'Sport kinds' }}
        </button>
      </div>
    </div>

    <ActivityKindsPanel v-if="tab === 'sport'" />

    <template v-else>
      <div class="flex flex-wrap items-center gap-3">
        <input
          v-model="q"
          class="w-64"
          placeholder="Search name or alias"
          data-testid="food-search"
        />
        <label class="inline-flex items-center gap-1 text-xs text-muted">
          <input v-model="includeArchived" type="checkbox" /> archived too
        </label>
        <span class="flex-1"></span>
        <button class="btn" data-testid="add-food" @click="startAdd">+ Add food</button>
      </div>
      <p class="text-sm text-muted">
        A saved food is a named number per portion, shared by the household, so the same dish always
        counts the same. Editing one does not change meals already logged.
      </p>
      <p v-if="error" class="note-danger">{{ error }}</p>

      <div class="card overflow-x-auto">
        <table class="w-full text-sm">
          <thead class="table-head">
            <tr>
              <th class="px-3 py-2">Name</th>
              <th class="px-3 py-2">Portion</th>
              <th class="px-3 py-2 text-right">kcal</th>
              <th class="px-3 py-2 text-right">Protein</th>
              <th class="px-3 py-2 text-right">Used</th>
              <th class="px-3 py-2"></th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-if="adding"
              class="bg-accent-soft"
              data-testid="food-editor"
              @keydown.enter="save"
            >
              <td class="px-2 py-1">
                <input
                  v-model="form.name"
                  class="w-full"
                  placeholder="Name"
                  data-testid="food-name"
                />
                <input
                  v-model="form.aliases"
                  class="mt-1 w-full text-xs"
                  placeholder="aliases, comma separated"
                />
              </td>
              <td class="px-2 py-1">
                <input v-model="form.portion" class="w-full" placeholder="1 bowl (350 g)" />
              </td>
              <td class="px-2 py-1">
                <input
                  v-model="form.kcal"
                  class="w-20 text-right"
                  inputmode="numeric"
                  placeholder="kcal"
                />
              </td>
              <td class="px-2 py-1">
                <input
                  v-model="form.protein_g"
                  class="w-16 text-right"
                  inputmode="numeric"
                  placeholder="g"
                />
              </td>
              <td></td>
              <td class="px-2 py-1 text-right whitespace-nowrap">
                <button
                  class="btn"
                  :disabled="!valid() || busy"
                  data-testid="food-save"
                  @click="save"
                >
                  Add
                </button>
                <button class="btn-secondary ml-1" @click="adding = false">Cancel</button>
              </td>
            </tr>
            <template v-for="f in foods" :key="f.id">
              <tr
                v-if="editing === f.id"
                class="bg-accent-soft"
                data-testid="food-editor"
                @keydown.enter="save"
                @keydown.esc="editing = null"
              >
                <td class="px-2 py-1">
                  <input v-model="form.name" class="w-full" />
                  <input
                    v-model="form.aliases"
                    class="mt-1 w-full text-xs"
                    placeholder="aliases, comma separated"
                  />
                </td>
                <td class="px-2 py-1"><input v-model="form.portion" class="w-full" /></td>
                <td class="px-2 py-1">
                  <input v-model="form.kcal" class="w-20 text-right" inputmode="numeric" />
                </td>
                <td class="px-2 py-1">
                  <input v-model="form.protein_g" class="w-16 text-right" inputmode="numeric" />
                </td>
                <td class="px-3 py-2 text-right tabular-nums">{{ f.uses }}</td>
                <td class="px-2 py-1 text-right whitespace-nowrap">
                  <button
                    class="btn"
                    :disabled="!valid() || busy"
                    data-testid="food-save"
                    @click="save"
                  >
                    Save
                  </button>
                  <button class="btn-secondary ml-1" @click="editing = null">Cancel</button>
                </td>
              </tr>
              <tr
                v-else
                class="border-t border-edge"
                :class="{ 'text-faint': f.archived_at }"
                data-testid="food-row"
                title="Double-click to edit"
                @dblclick="startEdit(f)"
              >
                <td class="px-3 py-2">
                  {{ f.name }}
                  <span v-for="a in f.aliases" :key="a" class="chip ml-1">{{ a }}</span>
                  <span v-if="f.archived_at" class="chip ml-1">archived</span>
                </td>
                <td class="px-3 py-2 text-muted">{{ f.portion }}</td>
                <td class="px-3 py-2 text-right tabular-nums">{{ f.kcal }}</td>
                <td class="px-3 py-2 text-right tabular-nums">{{ f.protein_g ?? '—' }}</td>
                <td class="px-3 py-2 text-right tabular-nums">{{ f.uses }}</td>
                <td class="px-3 py-2 text-right whitespace-nowrap">
                  <button class="link text-xs" @click="startEdit(f)">edit</button>
                  <button
                    class="link ml-2 text-xs"
                    :class="f.archived_at ? '' : 'text-danger'"
                    @click="archive(f)"
                  >
                    {{ f.archived_at ? 'restore' : 'archive' }}
                  </button>
                </td>
              </tr>
            </template>
            <tr v-if="!foods.length && !adding">
              <td colspan="6" class="px-3 py-6 text-center text-muted">No foods yet.</td>
            </tr>
          </tbody>
        </table>
      </div>
    </template>
  </main>
</template>
