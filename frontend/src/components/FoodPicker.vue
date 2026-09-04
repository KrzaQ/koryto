<script setup lang="ts">
// A search box over the household's foods. Picking one emits it; clearing
// the box emits null so the row goes back to free-text numbers.
import { computed, ref, watch } from 'vue'
import { api } from '@/api/client'
import type { FoodDto } from '@/api/types'

const props = defineProps<{ selected: FoodDto | null; placeholder?: string }>()
const emit = defineEmits<{ pick: [food: FoodDto | null] }>()

const query = ref(props.selected?.name ?? '')
const hits = ref<FoodDto[]>([])
const open = ref(false)
let timer: ReturnType<typeof setTimeout> | undefined

watch(
  () => props.selected,
  (f) => {
    query.value = f?.name ?? ''
  },
)

async function search() {
  try {
    hits.value = await api.foods.list(query.value.trim())
  } catch {
    hits.value = []
  }
}

function onInput() {
  open.value = true
  if (props.selected && query.value !== props.selected.name) emit('pick', null)
  clearTimeout(timer)
  timer = setTimeout(search, 120)
}

function pick(f: FoodDto) {
  query.value = f.name
  open.value = false
  emit('pick', f)
}

function closeSoon() {
  setTimeout(() => (open.value = false), 150)
}

function clear() {
  query.value = ''
  open.value = false
  emit('pick', null)
}

const shown = computed(() => hits.value.slice(0, 8))
</script>

<template>
  <div class="relative">
    <input
      v-model="query"
      class="w-full"
      :class="{ 'border-accent': selected }"
      :placeholder="placeholder ?? 'Saved food'"
      autocomplete="off"
      data-testid="food-picker"
      @input="onInput"
      @focus="((open = true), search())"
      @blur="closeSoon"
      @keydown.esc="open = false"
    />
    <button
      v-if="selected"
      type="button"
      class="absolute top-1/2 right-1 -translate-y-1/2 text-xs text-muted hover:text-fg"
      title="Unlink the food"
      @click="clear"
    >
      ×
    </button>
    <ul
      v-if="open && shown.length"
      class="absolute z-10 mt-1 max-h-64 w-full overflow-auto rounded border border-edge bg-surface text-sm shadow-lg"
      data-testid="food-hits"
    >
      <li v-for="f in shown" :key="f.id">
        <button
          type="button"
          class="flex w-full items-baseline gap-2 px-2 py-1 text-left hover:bg-surface-2"
          @mousedown.prevent="pick(f)"
        >
          <span class="flex-1">{{ f.name }}</span>
          <span class="text-xs text-muted">{{ f.portion }}</span>
          <span class="font-mono text-xs tabular-nums">{{ f.kcal }} kcal</span>
        </button>
      </li>
    </ul>
  </div>
</template>
