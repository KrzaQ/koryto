import { createRouter, createWebHistory } from 'vue-router'
import { isDay } from '@/lib/day'
import { useSession } from '@/stores/session'

const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: '/', name: 'home', component: () => import('@/views/HomeView.vue') },
    {
      path: '/login',
      name: 'login',
      component: () => import('@/views/LoginView.vue'),
      meta: { public: true },
    },
    {
      path: '/d/:day',
      name: 'day',
      component: () => import('@/views/DayView.vue'),
      props: true,
    },
    { path: '/trends', name: 'trends', component: () => import('@/views/TrendsView.vue') },
    { path: '/foods', name: 'foods', component: () => import('@/views/FoodsView.vue') },
    { path: '/profile', name: 'profile', component: () => import('@/views/ProfileView.vue') },
    {
      path: '/tokens',
      name: 'tokens',
      component: () => import('@/views/TokensView.vue'),
    },
    {
      path: '/:pathMatch(.*)*',
      name: 'not-found',
      component: () => import('@/views/NotFoundView.vue'),
    },
  ],
})

router.beforeEach(async (to) => {
  if (to.meta.public) return true
  const session = useSession()
  if (!session.checked) await session.load()
  if (!session.me) return { name: 'login', query: { next: to.fullPath } }
  // "today" and bad dates are resolved here rather than in a beforeEnter,
  // which does not run when only the :day parameter changes.
  if (to.name === 'day') {
    const d = String(to.params.day)
    if (d === 'today') return `/d/${session.today}`
    if (!isDay(d)) return `/d/${session.today}`
  }
  return true
})

export default router
