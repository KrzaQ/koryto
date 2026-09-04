<script setup lang="ts">
// The mark: a rounded tile with a trough, which is what koryto means.
// Same drawing as public/favicon.svg.
withDefaults(defineProps<{ size?: number; wordmark?: boolean }>(), { size: 28, wordmark: true })
</script>

<template>
  <span class="inline-flex items-center gap-2">
    <svg
      :width="size"
      :height="size"
      viewBox="0 0 64 64"
      xmlns="http://www.w3.org/2000/svg"
      aria-hidden="true"
    >
      <defs>
        <linearGradient id="koryto-tile" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stop-color="#eb6834" />
          <stop offset="1" stop-color="#eda100" />
        </linearGradient>
      </defs>
      <rect x="2" y="2" width="60" height="60" rx="14" fill="url(#koryto-tile)" />
      <path
        d="M12 24h40l-6 20H18z"
        fill="none"
        stroke="#ffffff"
        stroke-width="5"
        stroke-linejoin="round"
      />
      <path
        d="M22 24v-6M42 24v-6"
        stroke="#ffffff"
        stroke-width="4"
        stroke-linecap="round"
        opacity="0.85"
      />
    </svg>
    <span v-if="wordmark" class="text-lg font-semibold tracking-tight text-fg">koryto</span>
  </span>
</template>
