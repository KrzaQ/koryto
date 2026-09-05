<script setup lang="ts">
import { useRouter } from 'vue-router'
import AppLogo from '@/components/AppLogo.vue'
import PersonChooser from '@/components/PersonChooser.vue'
import ThemeChooser from '@/components/ThemeChooser.vue'
import ZoneChooser from '@/components/ZoneChooser.vue'
import { useSession } from '@/stores/session'

const session = useSession()
const router = useRouter()

const links = [
  { to: '/', label: 'Home', match: '/home' },
  { to: '/d/today', label: 'Day', match: '/d' },
  { to: '/trends', label: 'Trends', match: '/trends' },
  { to: '/foods', label: 'Reference', match: '/foods' },
  { to: '/profile', label: 'Profile', match: '/profile' },
  { to: '/tokens', label: 'Tokens', match: '/tokens' },
]

async function logout() {
  await session.logout()
  router.push({ name: 'login' })
}
</script>

<template>
  <header class="border-b border-edge bg-surface">
    <nav class="mx-auto flex max-w-6xl flex-wrap items-center gap-x-5 gap-y-2 px-4 py-2.5">
      <RouterLink to="/" class="mr-2"><AppLogo :size="26" /></RouterLink>
      <RouterLink
        v-for="l in links"
        :key="l.label"
        :to="l.to"
        class="text-sm text-muted hover:text-fg"
        :class="{ 'font-medium text-fg': $route.path === l.to || $route.path.startsWith(l.match) }"
      >
        {{ l.label }}
      </RouterLink>
      <span class="flex-1"></span>
      <PersonChooser />
      <ZoneChooser />
      <ThemeChooser />
      <span v-if="session.me" class="text-sm text-muted">{{
        session.me.user.name ?? session.me.user.email
      }}</span>
      <button class="text-sm text-muted hover:text-fg" @click="logout">Log out</button>
    </nav>
  </header>
</template>
