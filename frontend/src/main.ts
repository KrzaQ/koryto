import { createApp } from 'vue'
import { createPinia } from 'pinia'
import App from './App.vue'
import router from './router'
import './style.css'
import { useTheme } from './stores/theme'

const app = createApp(App).use(createPinia()).use(router)
useTheme() // stamps data-theme on <html> and keeps it in sync
app.mount('#app')
