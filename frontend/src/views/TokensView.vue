<script setup lang="ts">
import { computed, onMounted, reactive, ref } from 'vue'
import { api } from '@/api/client'
import type { TokenCreated, TokenDto } from '@/api/types'
import { formatDateTime } from '@/lib/time'
import { useSession } from '@/stores/session'
import { useTimezone } from '@/stores/timezone'

const session = useSession()
const tokens = ref<TokenDto[]>([])
const created = ref<TokenCreated | null>(null)
const error = ref<string | null>(null)
const form = reactive({ name: '', level: 'read,write,edit', delegate: false, user_id: 0 })
const scopes = computed(() => (form.delegate ? `${form.level},delegate` : form.level))

async function load() {
  tokens.value = await api.tokens.list()
}

async function create() {
  error.value = null
  try {
    created.value = await api.tokens.create({
      name: form.name,
      scopes: scopes.value,
      user_id: form.delegate ? undefined : form.user_id || undefined,
    })
    form.name = ''
    await load()
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  }
}

async function revoke(t: TokenDto) {
  if (!window.confirm(`Revoke "${t.name}"? Clients using it stop working immediately.`)) return
  await api.tokens.revoke(t.id)
  await load()
}

const tz = useTimezone()
function when(s?: string | null) {
  return s ? formatDateTime(s, tz.zone) : '—'
}
function who(t: TokenDto) {
  if (t.user_id == null) return 'delegate'
  const m = session.members.find((m) => m.id === t.user_id)
  return m?.name ?? m?.email ?? `#${t.user_id}`
}

onMounted(() => {
  form.user_id = session.me?.user.id ?? 0
  load()
})
</script>

<template>
  <main class="mx-auto max-w-4xl space-y-6 px-4 py-6">
    <h1 class="text-2xl font-semibold">API tokens</h1>
    <p class="text-sm text-muted">
      Bearer tokens for MCP clients. <code>read</code> looks; <code>write</code> logs meals, weights
      and sport, adds foods and sets locations; <code>edit</code> changes and voids entries,
      targets, foods and the profile. A personal token acts as one person. A
      <code>delegate</code> token is for a trusted gateway such as Open WebUI: every request names
      the acting person's email in <code>X-Koryto-User</code>, and that person must have logged in
      here before.
    </p>
    <p v-if="error" class="note-danger">{{ error }}</p>

    <div v-if="created" class="note-ok" data-testid="token-secret">
      <p class="font-medium text-ok">
        Token "{{ created.name }}" created. Copy it now; it will not be shown again.
      </p>
      <code
        class="mt-2 block rounded bg-surface px-3 py-2 font-mono text-xs break-all select-all"
        >{{ created.secret }}</code
      >
      <button class="btn-secondary mt-3" @click="created = null">Done</button>
    </div>

    <div class="overflow-x-auto card">
      <table class="w-full text-sm">
        <thead class="table-head">
          <tr>
            <th class="px-3 py-2">Name</th>
            <th class="px-3 py-2">Acts as</th>
            <th class="px-3 py-2">Scopes</th>
            <th class="px-3 py-2">Created</th>
            <th class="px-3 py-2">Last used</th>
            <th class="px-3 py-2"></th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="t in tokens"
            :key="t.id"
            class="border-t border-edge"
            :class="{ 'text-faint line-through': t.revoked_at }"
            data-testid="token-row"
          >
            <td class="px-3 py-2">{{ t.name }}</td>
            <td class="px-3 py-2">{{ who(t) }}</td>
            <td class="px-3 py-2 font-mono text-xs">{{ t.scopes.join(',') }}</td>
            <td class="px-3 py-2 font-mono text-xs">{{ when(t.created_at) }}</td>
            <td class="px-3 py-2 font-mono text-xs">{{ when(t.last_used_at) }}</td>
            <td class="px-3 py-2 text-right">
              <button v-if="!t.revoked_at" class="link text-xs text-danger" @click="revoke(t)">
                Revoke
              </button>
            </td>
          </tr>
          <tr v-if="tokens.length === 0">
            <td colspan="6" class="px-3 py-6 text-center text-muted">No tokens yet.</td>
          </tr>
        </tbody>
      </table>
    </div>

    <form class="flex flex-wrap items-end gap-3 card p-4" @submit.prevent="create">
      <label class="text-sm"
        >Name<br /><input v-model="form.name" required class="w-48" placeholder="claude-code"
      /></label>
      <label class="text-sm"
        >Scopes<br />
        <select v-model="form.level">
          <option value="read">read</option>
          <option value="read,write">read,write</option>
          <option value="read,write,edit">read,write,edit</option>
        </select>
      </label>
      <label class="flex items-center gap-2 pb-1.5 text-sm"
        ><input v-model="form.delegate" type="checkbox" /> delegate</label
      >
      <label v-if="!form.delegate && session.members.length > 1" class="text-sm"
        >Acts as<br />
        <select v-model="form.user_id">
          <option v-for="m in session.members" :key="m.id" :value="m.id">
            {{ m.name ?? m.email }}
          </option>
        </select>
      </label>
      <button class="btn" type="submit">Create token</button>
    </form>
  </main>
</template>
