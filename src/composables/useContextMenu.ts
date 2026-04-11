import { ref, reactive } from 'vue'

export interface MenuItem {
  label?: string
  icon?: string
  action?: () => void
  divider?: boolean
  danger?: boolean
}

const isVisible = ref(false)
const position = reactive({ x: 0, y: 0 })
const menuItems = ref<MenuItem[]>([])

export function useContextMenu() {
  const showContextMenu = (event: MouseEvent, items: MenuItem[]) => {
    event.preventDefault()
    position.x = event.clientX
    position.y = event.clientY
    menuItems.value = items
    isVisible.value = true
  }

  const hideContextMenu = () => {
    isVisible.value = false
    menuItems.value = []
  }

  return {
    isVisible,
    position,
    menuItems,
    showContextMenu,
    hideContextMenu
  }
}
