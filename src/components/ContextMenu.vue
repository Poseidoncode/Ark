<script setup lang="ts">
import { ref, watch, onMounted, onUnmounted, nextTick } from 'vue'

export interface MenuItem {
  label?: string
  icon?: string
  action?: () => void
  divider?: boolean
  danger?: boolean
}

const props = defineProps<{
  items: MenuItem[]
  position: { x: number; y: number }
  visible: boolean
}>()

const emit = defineEmits<{
  (e: 'close'): void
}>()

const menuRef = ref<HTMLElement | null>(null)
const menuStyles = ref({ top: '0px', left: '0px' })
const focusedIndex = ref(-1)

const calculatePosition = async () => {
  if (!props.visible) return
  
  await nextTick()
  if (!menuRef.value) return

  const menuRect = menuRef.value.getBoundingClientRect()
  const windowWidth = window.innerWidth
  const windowHeight = window.innerHeight

  let newX = props.position.x
  let newY = props.position.y

  // Prevent overflow on right
  if (newX + menuRect.width > windowWidth) {
    newX = windowWidth - menuRect.width - 8
  }
  
  // Prevent overflow on bottom
  if (newY + menuRect.height > windowHeight) {
    newY = windowHeight - menuRect.height - 8
  }

  // Prevent overflow on left/top (fallback)
  if (newX < 8) newX = 8
  if (newY < 8) newY = 8

  menuStyles.value = {
    top: `${newY}px`,
    left: `${newX}px`
  }
}

watch([() => props.position.x, () => props.position.y, () => props.visible], () => {
  if (props.visible) {
    focusedIndex.value = -1
    calculatePosition()
  }
})

const handleClickOutside = (e: MouseEvent) => {
  if (props.visible && menuRef.value && !menuRef.value.contains(e.target as Node)) {
    emit('close')
  }
}

const handleKeyDown = (e: KeyboardEvent) => {
  if (!props.visible) return

  const items = props.items
  const selectableItems = items.map((item, index) => ({ item, index })).filter(x => !x.item.divider)
  
  if (selectableItems.length === 0) return

  const currentIndex = selectableItems.findIndex(x => x.index === focusedIndex.value)

  switch (e.key) {
    case 'Escape':
      emit('close')
      break
    case 'ArrowDown':
      e.preventDefault()
      if (currentIndex < selectableItems.length - 1) {
        focusedIndex.value = selectableItems[currentIndex + 1].index
      } else {
        focusedIndex.value = selectableItems[0].index
      }
      break
    case 'ArrowUp':
      e.preventDefault()
      if (currentIndex > 0) {
        focusedIndex.value = selectableItems[currentIndex - 1].index
      } else {
        focusedIndex.value = selectableItems[selectableItems.length - 1].index
      }
      break
    case 'Enter':
      e.preventDefault()
      if (focusedIndex.value !== -1 && !items[focusedIndex.value].divider) {
        executeAction(items[focusedIndex.value])
      }
      break
  }
}

const executeAction = (item: MenuItem) => {
  if (item.action) {
    item.action()
  }
  emit('close')
}

onMounted(() => {
  document.addEventListener('mousedown', handleClickOutside)
  document.addEventListener('keydown', handleKeyDown)
})

onUnmounted(() => {
  document.removeEventListener('mousedown', handleClickOutside)
  document.removeEventListener('keydown', handleKeyDown)
})
</script>

<template>
  <Transition name="context-menu">
    <div
      v-if="visible"
      ref="menuRef"
      class="fixed z-50 min-w-[200px] py-1 bg-card dark:bg-[#1e1e1e] border border-border dark:border-[#333] rounded-lg shadow-xl outline-none"
      :style="menuStyles"
      role="menu"
      tabindex="-1"
    >
      <template v-for="(item, index) in items" :key="index">
        <!-- Divider -->
        <div 
          v-if="item.divider" 
          class="h-px bg-border dark:bg-[#333] my-1 mx-2"
          role="separator"
        ></div>
        
        <!-- Menu Item -->
        <button
          v-else
          class="w-full text-left px-3 py-1.5 flex items-center space-x-2 text-sm transition-colors outline-none"
          :class="[
            item.danger ? 'text-red-500 hover:bg-red-500/10 focus:bg-red-500/10' : 'text-foreground hover:bg-muted focus:bg-muted',
            focusedIndex === index ? (item.danger ? 'bg-red-500/10' : 'bg-muted') : ''
          ]"
          @click="executeAction(item)"
          @mouseenter="focusedIndex = index"
          role="menuitem"
        >
          <span v-if="item.icon" class="w-4 h-4 flex items-center justify-center opacity-70">
            <!-- Icon placeholder -->
            <i :class="item.icon"></i>
          </span>
          <span class="flex-1 truncate">{{ item.label }}</span>
        </button>
      </template>
    </div>
  </Transition>
</template>

<style scoped>
.context-menu-enter-active,
.context-menu-leave-active {
  transition: opacity 0.15s ease, transform 0.15s ease;
}

.context-menu-enter-from,
.context-menu-leave-to {
  opacity: 0;
  transform: scale(0.95);
}
</style>
