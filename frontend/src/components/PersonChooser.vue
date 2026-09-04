<script setup lang="ts">
import { usePerson } from '@/stores/person'
import { useSession } from '@/stores/session'

const session = useSession()
const person = usePerson()
</script>

<template>
  <div
    v-if="session.members.length > 1"
    class="inline-flex overflow-hidden rounded border border-edge text-xs"
    role="radiogroup"
    aria-label="Person"
    data-testid="person-chooser"
  >
    <button
      v-for="m in session.members"
      :key="m.id"
      type="button"
      role="radio"
      :aria-checked="person.id === m.id"
      :title="m.email ?? undefined"
      class="px-2 py-1"
      :class="
        person.id === m.id ? 'bg-accent text-white' : 'bg-surface text-muted hover:bg-surface-2'
      "
      @click="person.set(m.id)"
    >
      {{ m.name ?? m.email }}
    </button>
  </div>
</template>
