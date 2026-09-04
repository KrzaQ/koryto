<script setup lang="ts">
import { useSession } from '@/stores/session'

const session = useSession()
</script>

<template>
  <main class="mx-auto max-w-2xl px-4 py-12">
    <h1 class="text-2xl font-semibold">You are in, but not in a household yet</h1>
    <p class="mt-3 text-sm text-muted">
      Logged in as <b>{{ session.me?.user.email ?? session.me?.user.name }}</b
      >. Someone with access to the server puts you in a household from the terminal:
    </p>
    <pre class="mt-3 rounded bg-surface-2 p-3 font-mono text-xs">
koryto household add-member &lt;household&gt; {{ session.me?.user.email }}</pre>
    <p class="mt-3 text-sm text-muted">Reload this page afterwards.</p>
  </main>
</template>
