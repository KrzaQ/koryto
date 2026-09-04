<script setup lang="ts">
// One meal row in edit mode, or the add row. A picked food fills the numbers
// (times portions); typing a number by hand unlinks it. Emits the wire shape.
import { computed, reactive, ref } from 'vue'
import type { FoodDto, MealDto, MealInput, MealPatchInput } from '@/api/types'
import { looksLikePortions } from '@/lib/units'
import { fromLocalInput, nowLocal, toLocalInput } from '@/lib/time'
import { useTimezone } from '@/stores/timezone'
import FoodPicker from './FoodPicker.vue'

const props = defineProps<{
  meal?: MealDto
  /** The day the add row is on, YYYY-MM-DD; default now. */
  day?: string
  /** Everyone the add row may log for, when there is more than one. */
  members?: { id: number; name: string }[]
  userId: number
  busy?: boolean
}>()
const emit = defineEmits<{
  create: [input: MealInput]
  save: [patch: MealPatchInput]
  cancel: []
  void: []
}>()

const tz = useTimezone()
const SOURCES = ['estimate', 'manual', 'label'] as const

function initialTime(): string {
  if (props.meal) return toLocalInput(props.meal.eaten_at, tz.zone)
  const now = nowLocal(tz.zone)
  if (!props.day || now.startsWith(props.day)) return now
  return `${props.day}T12:00`
}

const food = ref<FoodDto | null>(null)
const form = reactive({
  eaten_at: initialTime(),
  description: props.meal?.description ?? '',
  kcal: props.meal?.kcal?.toString() ?? '',
  protein_g: props.meal?.protein_g?.toString() ?? '',
  source: props.meal?.source && props.meal.source !== 'food' ? props.meal.source : 'estimate',
  portions: props.meal?.portions ?? '1',
  linked: !!props.meal?.food_id,
  for: props.meal ? [props.meal.user_id] : [props.userId],
})

function pick(f: FoodDto | null) {
  food.value = f
  form.linked = !!f
  if (f) {
    if (!form.description.trim() || form.description === food.value?.name) form.description = f.name
    applyPortions()
  }
}

function applyPortions() {
  if (!food.value || !looksLikePortions(form.portions)) return
  const p = Number(form.portions.replace(',', '.'))
  form.kcal = String(Math.round(food.value.kcal * p))
  form.protein_g = food.value.protein_g == null ? '' : String(Math.round(food.value.protein_g * p))
}

function typedNumber() {
  form.linked = false
  food.value = null
}

const valid = computed(
  () =>
    form.eaten_at.length >= 16 &&
    form.description.trim().length > 0 &&
    /^\d+$/.test(form.kcal.trim()) &&
    (form.protein_g.trim() === '' || /^\d+$/.test(form.protein_g.trim())) &&
    (!form.linked || looksLikePortions(form.portions)) &&
    form.for.length > 0,
)

function submit() {
  if (!valid.value) return
  const eaten_at = fromLocalInput(form.eaten_at, tz.zone)
  const protein = form.protein_g.trim() === '' ? null : Number(form.protein_g)
  if (props.meal) {
    const patch: MealPatchInput = { eaten_at, description: form.description.trim() }
    if (form.linked && food.value) {
      patch.food_id = food.value.id
      patch.portions = form.portions.trim()
    } else if (form.linked && props.meal.food_id) {
      // Still on the original food: only the portions may have changed.
      if (form.portions !== props.meal.portions) patch.portions = form.portions.trim()
    } else {
      patch.kcal = Number(form.kcal)
      patch.protein_g = protein
      patch.source = form.source
    }
    emit('save', patch)
  } else {
    const input: MealInput = {
      user_ids: form.for,
      eaten_at,
      description: form.description.trim(),
    }
    if (form.linked && food.value) {
      input.food_id = food.value.id
      input.portions = form.portions.trim()
    } else {
      input.kcal = Number(form.kcal)
      input.protein_g = protein
      input.source = form.source
    }
    emit('create', input)
  }
}
</script>

<template>
  <tr
    class="bg-accent-soft"
    data-testid="meal-editor"
    @keydown.enter.prevent="submit"
    @keydown.esc="emit('cancel')"
  >
    <td class="w-36 px-2 py-1">
      <input
        v-model="form.eaten_at"
        type="datetime-local"
        class="w-full font-mono text-xs"
        data-testid="editor-time"
      />
    </td>
    <td class="px-2 py-1">
      <div class="flex gap-2">
        <input
          v-model="form.description"
          class="flex-1"
          placeholder="What was eaten"
          data-testid="editor-description"
        />
        <div class="w-48">
          <FoodPicker
            :selected="
              food ??
              (meal?.food_id ? ({ id: meal.food_id, name: '(saved food)' } as FoodDto) : null)
            "
            @pick="pick"
          />
        </div>
        <input
          v-if="form.linked"
          v-model="form.portions"
          class="w-16 text-right"
          title="Portions"
          placeholder="1"
          data-testid="editor-portions"
          @input="applyPortions"
        />
      </div>
      <div v-if="members && !meal && members.length > 1" class="mt-1 flex gap-3 text-xs text-muted">
        <label v-for="m in members" :key="m.id" class="inline-flex items-center gap-1">
          <input v-model="form.for" type="checkbox" :value="m.id" /> {{ m.name }}
        </label>
      </div>
    </td>
    <td class="w-20 px-2 py-1 text-right">
      <input
        v-model="form.kcal"
        inputmode="numeric"
        class="w-full text-right"
        placeholder="kcal"
        :readonly="form.linked"
        :class="{ 'text-muted': form.linked }"
        data-testid="editor-kcal"
        @input="typedNumber"
      />
    </td>
    <td class="w-16 px-2 py-1 text-right">
      <input
        v-model="form.protein_g"
        inputmode="numeric"
        class="w-full text-right"
        placeholder="g"
        :readonly="form.linked"
        :class="{ 'text-muted': form.linked }"
        data-testid="editor-protein"
        @input="typedNumber"
      />
    </td>
    <td class="w-24 px-2 py-1">
      <span v-if="form.linked" class="chip">food</span>
      <select v-else v-model="form.source" class="w-full text-xs" data-testid="editor-source">
        <option v-for="s in SOURCES" :key="s" :value="s">{{ s }}</option>
      </select>
    </td>
  </tr>
  <tr class="bg-accent-soft" @keydown.esc="emit('cancel')">
    <td colspan="5" class="px-2 pb-2 text-right whitespace-nowrap">
      <button class="btn" :disabled="!valid || busy" data-testid="editor-save" @click="submit">
        {{ meal ? 'Save' : 'Add' }}
      </button>
      <button class="btn-secondary ml-1" :disabled="busy" @click="emit('cancel')">Cancel</button>
      <button
        v-if="meal"
        class="btn-secondary ml-1 text-danger"
        :disabled="busy"
        title="Hide this meal from every total"
        data-testid="editor-void"
        @click="emit('void')"
      >
        Void
      </button>
    </td>
  </tr>
</template>
