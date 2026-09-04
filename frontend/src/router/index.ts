import { createRouter, createWebHistory } from 'vue-router'
import { isDay } from '@/lib/day'
import { useSession } from '@/stores/session'

const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: '/', name: 'home', redirect: () => `/d/today` },
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
      beforeEnter: (to) => {
        const d = String(to.params.day)
        if (d === 'today') {
          const session = useSession()
          return `/d/${session.me?.today ?? new Date().toISOString().slice(0, 10)}`
        }
        return isDay(d) ? true : '/d/today'
      },
    },
    { path: '/trends', name: 'trends', component: () => import('@/views/TrendsView.vue') },
    { path: '/foods', name: 'foods', component: () => import('@/views/FoodsView.vue') },
    { path: '/profile', name: 'profile', component: () => import('@/views/ProfileView.vue') },
    {
      path: '/tokens',
      name: 'tokens',
      component: () => import('@/views/TokensView.vue'),
      meta: { withoutHousehold: true },
    },
    {
      path: '/welcome',
      name: 'welcome',
      component: () => import('@/views/NoHouseholdView.vue'),
      meta: { withoutHousehold: true },
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
  if (!session.inHousehold && !to.meta.withoutHousehold) return { name: 'welcome' }
  return true
})

export default router
