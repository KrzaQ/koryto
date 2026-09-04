<script setup lang="ts">
// The mark: a rounded tile with a fed trough, which is what koryto means.
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
      <g fill="#ffffff">
        <path d="M15 21C19 9 45 9 49 21Z" />
        <rect x="9" y="19" width="46" height="6" rx="2" />
        <path d="M13 25h38l-5 19H18z" />
        <rect x="17" y="44" width="6" height="9" rx="1.5" />
        <rect x="41" y="44" width="6" height="9" rx="1.5" />
      </g>
    </svg>
    <span v-if="wordmark" class="text-lg font-semibold tracking-tight text-fg">koryto</span>
  </span>
</template>
