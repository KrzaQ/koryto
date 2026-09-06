<script setup lang="ts">
import { useRoute } from 'vue-router'
import AppNav from '@/components/AppNav.vue'
import { reloadKey } from '@/lib/reload'

const route = useRoute()
</script>

<template>
  <div class="min-h-screen bg-canvas text-fg">
    <AppNav v-if="!route.meta.public" />
    <RouterView :key="reloadKey" />
  </div>
</template>
