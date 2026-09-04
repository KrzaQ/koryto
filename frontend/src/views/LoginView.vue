<script setup lang="ts">
import { computed } from 'vue'
import { useRoute } from 'vue-router'

const route = useRoute()
const next = computed(() => {
  const n = String(route.query.next ?? '/')
  return n.startsWith('/') && !n.startsWith('//') ? n : '/'
})
const href = computed(() => `/api/auth/login?next=${encodeURIComponent(next.value)}`)
</script>

<template>
  <main class="flex min-h-screen items-center justify-center">
    <div class="w-80 card p-8 text-center shadow-sm">
      <h1 class="text-2xl font-semibold text-accent">koryto</h1>
      <p class="mt-2 text-sm text-muted">Meals, weight and sport for the household.</p>
      <a :href="href" class="btn mt-6 w-full justify-center" data-testid="login-button">Log in</a>
    </div>
  </main>
</template>
