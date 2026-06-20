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

const calculatePosition = async () => {
  if (!props.visible) return
  await nextTick()
  if (!menuRef.value) return

  const menuRect = menuRef.value.getBoundingClientRect()
  const windowWidth = window.innerWidth
  const windowHeight = window.innerHeight

  let newX = props.position.x
  let newY = props.position.y

  if (newX + menuRect.width > windowWidth) newX = windowWidth - menuRect.width - 8
  if (newY + menuRect.height > windowHeight) newY = windowHeight - menuRect.height - 8
  if (newX < 8) newX = 8
  if (newY < 8) newY = 8

  menuStyles.value = { top: `${newY}px`, left: `${newX}px` }
}

watch([() => props.position.x, () => props.position.y, () => props.visible], () => {
  if (props.visible) calculatePosition()
})

const handleClickOutside = (e: MouseEvent) => {
  if (props.visible && menuRef.value && !menuRef.value.contains(e.target as Node)) {
    emit('close')
  }
}

const executeAction = (item: MenuItem) => {
  if (item.action) item.action()
  emit('close')
}

onMounted(() => {
  document.addEventListener('mousedown', handleClickOutside)
})
onUnmounted(() => {
  document.removeEventListener('mousedown', handleClickOutside)
})
</script>

<template>
  <Transition name="context-menu">
    <div
      v-if="visible"
      ref="menuRef"
      class="fixed z-50 min-w-[200px] py-1.5 rounded-xl outline-none glass shadow-xl"
      :style="menuStyles"
      role="menu"
      tabindex="-1"
    >
      <template v-for="(item, index) in items" :key="index">
        <div
          v-if="item.divider"
          class="h-px my-1.5 mx-3"
          style="background: var(--border);"
          role="separator"
        ></div>
        <button
          v-else
          class="text-left flex items-center gap-2.5 text-[13px] outline-none rounded-lg transition-safe font-medium"
          style="width: calc(100% - 12px); margin: 1px 6px; padding: 6px 10px;"
          :class="item.danger ? 'hover:bg-red-500/10 focus:bg-red-500/10' : 'hover:bg-muted focus:bg-muted'"
          :style="{ color: item.danger ? 'var(--error)' : 'var(--foreground)' }"
          @click="executeAction(item)"
          role="menuitem"
        >
          <span v-if="item.icon" class="w-4 h-4 flex items-center justify-center opacity-50 flex-shrink-0">
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
  transition: opacity 0.15s cubic-bezier(0.4, 0, 0.2, 1), transform 0.15s cubic-bezier(0.16, 1, 0.3, 1);
}

.context-menu-enter-from,
.context-menu-leave-to {
  opacity: 0;
  transform: scale(0.96) translateY(-4px);
}
</style>
