<script setup lang="ts">
import { useRouter } from 'vue-router'
import { NavigationFailureType, isNavigationFailure } from 'vue-router'
import AppLogo from '@/components/AppLogo.vue'
import PersonChooser from '@/components/PersonChooser.vue'
import ThemeChooser from '@/components/ThemeChooser.vue'
import ZoneChooser from '@/components/ZoneChooser.vue'
import { bumpReload } from '@/lib/reload'
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

/**
 * Going nowhere is the interesting case: the router reports a duplicated
 * navigation, and that is the click that asks for the page again. A modified
 * click (new tab, new window) never reaches here; `navigate` lets it be.
 */
async function follow(e: MouseEvent, navigate: (e: MouseEvent) => Promise<unknown>) {
  const failure = await navigate(e)
  if (isNavigationFailure(failure, NavigationFailureType.duplicated)) {
    session.load()
    bumpReload()
  }
}

async function logout() {
  await session.logout()
  router.push({ name: 'login' })
}
</script>

<template>
  <header class="border-b border-edge bg-surface">
    <nav class="mx-auto flex max-w-6xl flex-wrap items-center gap-x-5 gap-y-2 px-4 py-2.5">
      <RouterLink v-slot="{ href, navigate }" to="/" custom>
        <a :href="href" class="mr-2" @click="follow($event, navigate)"><AppLogo :size="26" /></a>
      </RouterLink>
      <RouterLink v-for="l in links" :key="l.label" v-slot="{ href, navigate }" :to="l.to" custom>
        <a
          :href="href"
          class="text-sm text-muted hover:text-fg"
          :class="{
            'font-medium text-fg': $route.path === l.to || $route.path.startsWith(l.match),
          }"
          @click="follow($event, navigate)"
        >
          {{ l.label }}
        </a>
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
