<script setup lang="ts">
// A destructive action that asks first: the first click arms it, the second
// does it. Cheaper than a dialog and it keeps the row where it was. Arming
// lapses on blur or after a few seconds, so a stray click is harmless.
import { onUnmounted, ref } from 'vue'

const props = withDefaults(
  defineProps<{ label?: string; confirmLabel?: string; timeout?: number }>(),
  { label: 'void', confirmLabel: 'sure?', timeout: 4000 },
)
const emit = defineEmits<{ confirm: [] }>()

const armed = ref(false)
let timer: ReturnType<typeof setTimeout> | undefined

function disarm() {
  armed.value = false
  clearTimeout(timer)
}
function click() {
  if (armed.value) {
    disarm()
    emit('confirm')
    return
  }
  armed.value = true
  timer = setTimeout(disarm, props.timeout)
}
onUnmounted(() => clearTimeout(timer))
</script>

<template>
  <button
    :class="armed ? 'font-medium' : ''"
    :data-armed="armed || undefined"
    @click="click"
    @blur="disarm"
    @keydown.esc="disarm"
  >
    {{ armed ? confirmLabel : label }}
  </button>
</template>
