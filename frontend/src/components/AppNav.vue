<script setup lang="ts">
import { useRouter } from 'vue-router'
import AppLogo from '@/components/AppLogo.vue'
import ThemeChooser from '@/components/ThemeChooser.vue'
import { useSession } from '@/stores/session'

const session = useSession()
const router = useRouter()

const links = [{ to: '/', label: 'Day', exact: true }]

async function logout() {
  await session.logout()
  router.push({ name: 'login' })
}
</script>

<template>
  <header class="border-b border-edge bg-surface">
    <nav class="mx-auto flex max-w-6xl items-center gap-5 px-4 py-2.5">
      <RouterLink to="/" class="mr-2"><AppLogo :size="26" /></RouterLink>
      <RouterLink
        v-for="l in links"
        :key="l.label"
        :to="l.to"
        class="text-sm text-muted hover:text-fg"
        :class="{
          'font-medium text-fg': l.exact
            ? $route.path === '/'
            : $route.path.startsWith(l.to.split('/').slice(0, 2).join('/')),
        }"
      >
        {{ l.label }}
      </RouterLink>
      <span class="flex-1"></span>
      <ThemeChooser />
      <span v-if="session.me" class="text-sm text-muted">{{
        session.me.name ?? session.me.email
      }}</span>
      <button class="text-sm text-muted hover:text-fg" @click="logout">Log out</button>
    </nav>
  </header>
</template>
