<script setup lang="ts">
// The MET table: what a kind of sport costs per kilogram per hour. Shared by
// everyone, and editing a rate leaves sessions already logged alone, exactly
// as editing a food leaves past meals alone.
import { onMounted, reactive, ref, watch } from 'vue'
import { ApiError, api } from '@/api/client'
import type { ActivityKindDto } from '@/api/types'
import ConfirmButton from '@/components/ConfirmButton.vue'

const kinds = ref<ActivityKindDto[]>([])
const q = ref('')
const includeArchived = ref(false)
const error = ref<string | null>(null)
const busy = ref(false)
const editing = ref<number | null>(null)
const adding = ref(false)
const form = reactive({ name: '', aliases: '', met: '', note: '' })

async function load() {
  error.value = null
  try {
    kinds.value = await api.activityKinds.list(q.value.trim(), includeArchived.value)
  } catch (e) {
    error.value = e instanceof ApiError ? e.message : String(e)
  }
}
function startAdd() {
  Object.assign(form, { name: '', aliases: '', met: '', note: '' })
  editing.value = null
  adding.value = true
}
function startEdit(k: ActivityKindDto) {
  Object.assign(form, {
    name: k.name,
    aliases: k.aliases.join(', '),
    met: k.met,
    note: k.note,
  })
  adding.value = false
  editing.value = k.id
}
function valid() {
  const met = Number(form.met.replace(',', '.'))
  return form.name.trim().length > 0 && met >= 1 && met <= 25
}
async function run(f: () => Promise<unknown>) {
  busy.value = true
  error.value = null
  try {
    await f()
    await load()
    adding.value = false
    editing.value = null
  } catch (e) {
    error.value = e instanceof ApiError ? e.message : String(e)
  } finally {
    busy.value = false
  }
}
function payload() {
  return {
    name: form.name.trim(),
    aliases: form.aliases
      .split(',')
      .map((a) => a.trim())
      .filter(Boolean),
    met: form.met.trim(),
    note: form.note.trim(),
  }
}
function save() {
  if (!valid()) return
  const id = editing.value
  run(() => (id ? api.activityKinds.update(id, payload()) : api.activityKinds.create(payload())))
}

watch([q, includeArchived], load)
onMounted(load)
</script>

<template>
  <section class="space-y-3" data-testid="kinds-panel">
    <p class="text-xs text-muted">
      A session's burn is (MET − 1) × your weight in kg × hours: the MET is how many times harder
      than lying still the activity is, and the −1 is there because your base expenditure already
      pays for the hour underneath it. Log sport without a kcal figure and these fill it in.
    </p>
    <div class="flex flex-wrap items-center gap-2">
      <input v-model="q" class="w-64" placeholder="Search kinds" data-testid="kind-search" />
      <label class="flex items-center gap-1 text-sm text-muted"
        ><input v-model="includeArchived" type="checkbox" /> archived</label
      >
      <span class="flex-1"></span>
      <button class="btn" :disabled="busy" data-testid="add-kind" @click="startAdd">
        Add a kind
      </button>
    </div>
    <p v-if="error" class="note-danger">{{ error }}</p>
    <div class="card overflow-x-auto">
      <table class="w-full text-sm">
        <thead class="table-head">
          <tr>
            <th class="px-3 py-2">Kind</th>
            <th class="px-3 py-2">Also called</th>
            <th class="px-3 py-2 text-right">MET</th>
            <th class="px-3 py-2">Assumes</th>
            <th class="px-3 py-2 text-right">Sessions</th>
            <th class="px-3 py-2"></th>
          </tr>
        </thead>
        <tbody>
          <tr v-if="adding" class="border-t border-edge bg-accent-soft">
            <td class="px-2 py-1"><input v-model="form.name" class="w-32" placeholder="walk" /></td>
            <td class="px-2 py-1">
              <input v-model="form.aliases" class="w-40" placeholder="spacer, walking" />
            </td>
            <td class="px-2 py-1">
              <input v-model="form.met" class="w-16 text-right" placeholder="3.5" />
            </td>
            <td class="px-2 py-1">
              <input v-model="form.note" class="w-full" placeholder="about 5 km/h, flat" />
            </td>
            <td></td>
            <td class="px-2 py-1 text-right whitespace-nowrap">
              <button
                class="btn"
                :disabled="!valid() || busy"
                data-testid="kind-save"
                @click="save"
              >
                Add
              </button>
              <button class="btn-secondary ml-1" @click="adding = false">Cancel</button>
            </td>
          </tr>
          <template v-for="k in kinds" :key="k.id">
            <tr v-if="editing === k.id" class="border-t border-edge bg-accent-soft">
              <td class="px-2 py-1"><input v-model="form.name" class="w-32" /></td>
              <td class="px-2 py-1"><input v-model="form.aliases" class="w-40" /></td>
              <td class="px-2 py-1"><input v-model="form.met" class="w-16 text-right" /></td>
              <td class="px-2 py-1"><input v-model="form.note" class="w-full" /></td>
              <td></td>
              <td class="px-2 py-1 text-right whitespace-nowrap">
                <button class="btn" :disabled="!valid() || busy" @click="save">Save</button>
                <button class="btn-secondary ml-1" @click="editing = null">Cancel</button>
              </td>
            </tr>
            <tr
              v-else
              class="border-t border-edge"
              :class="k.archived_at ? 'text-faint' : ''"
              data-testid="kind-row"
              title="Double-click to edit"
              @dblclick="startEdit(k)"
            >
              <td class="px-3 py-2">{{ k.name }}</td>
              <td class="px-3 py-2 text-muted">{{ k.aliases.join(', ') }}</td>
              <td class="px-3 py-2 text-right tabular-nums">{{ k.met }}</td>
              <td class="px-3 py-2 text-muted">{{ k.note }}</td>
              <td class="px-3 py-2 text-right tabular-nums">{{ k.uses || '' }}</td>
              <td class="px-3 py-2 text-right whitespace-nowrap">
                <button class="link text-xs" @click="startEdit(k)">edit</button>
                <ConfirmButton
                  v-if="!k.archived_at"
                  class="link ml-2 text-xs text-danger"
                  label="archive"
                  @confirm="run(() => api.activityKinds.archive(k.id))"
                />
                <button
                  v-else
                  class="link ml-2 text-xs"
                  @click="run(() => api.activityKinds.unarchive(k.id))"
                >
                  restore
                </button>
              </td>
            </tr>
          </template>
          <tr v-if="!kinds.length && !adding">
            <td colspan="6" class="px-3 py-6 text-center text-muted">No kinds match.</td>
          </tr>
        </tbody>
      </table>
    </div>
  </section>
</template>
