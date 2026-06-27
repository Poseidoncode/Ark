<script setup lang="ts">
import { ref, watch, nextTick, onMounted } from 'vue';

const props = defineProps<{
  visible: boolean;
  title: string;
  label?: string;
  placeholder?: string;
  defaultValue?: string;
  confirmText?: string;
  cancelText?: string;
  inputType?: 'text' | 'textarea';
  required?: boolean;
  secondLabel?: string;
  secondPlaceholder?: string;
  secondDefaultValue?: string;
}>();

const emit = defineEmits<{
  (e: 'confirm', value: string, secondValue?: string): void;
  (e: 'cancel'): void;
}>();

const inputValue = ref('');
const secondInputValue = ref('');
const inputRef = ref<HTMLInputElement | HTMLTextAreaElement | null>(null);

watch(() => props.visible, async (val) => {
  if (val) {
    inputValue.value = props.defaultValue || '';
    secondInputValue.value = props.secondDefaultValue || '';
    await nextTick();
    inputRef.value?.focus();
  }
});

const handleConfirm = () => {
  if (props.required && !inputValue.value.trim()) return;
  emit('confirm', inputValue.value.trim(), secondInputValue.value.trim() || undefined);
};

const handleCancel = () => {
  emit('cancel');
};

const handleKeydown = (e: KeyboardEvent) => {
  if (e.key === 'Escape') {
    handleCancel();
  } else if (e.key === 'Enter' && !e.shiftKey && props.inputType !== 'textarea') {
    e.preventDefault();
    handleConfirm();
  }
};

onMounted(() => {
  if (props.visible) {
    inputValue.value = props.defaultValue || '';
    secondInputValue.value = props.secondDefaultValue || '';
  }
});
</script>

<template>
  <div v-if="visible" class="fixed inset-0 flex items-center justify-center z-[110] p-4" style="background: rgba(0,0,0,0.65); backdrop-filter: blur(8px);" @click.self="handleCancel">
    <div class="bg-card rounded-2xl shadow-xl p-8 w-full max-w-md border border-border" @keydown="handleKeydown">
      <h2 class="text-2xl font-display mb-6 text-foreground">{{ title }}</h2>

      <div class="mb-5">
        <label v-if="label" class="block mb-2 text-sm font-medium text-foreground">{{ label }}</label>
        <input
          v-if="!inputType || inputType === 'text'"
          ref="inputRef"
          v-model="inputValue"
          :placeholder="placeholder"
          class="input"
        />
        <textarea
          v-else
          ref="inputRef"
          v-model="inputValue"
          :placeholder="placeholder"
          class="input resize-none"
          rows="3"
        />
      </div>

      <div v-if="secondLabel" class="mb-5">
        <label class="block mb-2 text-sm font-medium text-foreground">{{ secondLabel }}</label>
        <input
          v-model="secondInputValue"
          :placeholder="secondPlaceholder"
          class="input"
        />
      </div>

      <div class="flex justify-end gap-3">
        <button @click="handleCancel" class="btn btn-ghost px-6 py-2.5">{{ cancelText || 'Cancel' }}</button>
        <button @click="handleConfirm" :disabled="required && !inputValue.trim()" class="btn btn-primary px-6 py-2.5">{{ confirmText || 'Confirm' }}</button>
      </div>
    </div>
  </div>
</template>
