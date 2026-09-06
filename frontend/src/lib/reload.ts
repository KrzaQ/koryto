// Clicking the link for the page you are already on should fetch it again,
// the way a browser's own address bar does. Vue Router treats that as no
// navigation at all, so the view is remounted by changing its key instead.
import { ref } from 'vue'

export const reloadKey = ref(0)

export function bumpReload() {
  reloadKey.value += 1
}
