<script setup lang="ts">
import { onMounted, onUnmounted } from 'vue';
import HomePage from './pages/HomePage.vue';

function clearTextSelection() {
  const sel = window.getSelection?.();
  if (sel && sel.rangeCount) sel.removeAllRanges();
}

function isButtonTarget(target: EventTarget | null): boolean {
  return !!(target instanceof Element && target.closest('button'));
}

function onMouseDown(event: MouseEvent) {
  if (event.button !== 2) return;
  if (!isButtonTarget(event.target)) return;
  event.preventDefault();
  clearTextSelection();
}

function onSelectStart(event: Event) {
  if (!isButtonTarget(event.target)) return;
  event.preventDefault();
}

function onContextMenu(event: MouseEvent) {
  if (!isButtonTarget(event.target)) return;
  clearTextSelection();
}

onMounted(() => {
  document.addEventListener('mousedown', onMouseDown, true);
  document.addEventListener('selectstart', onSelectStart, true);
  document.addEventListener('contextmenu', onContextMenu, true);
});

onUnmounted(() => {
  document.removeEventListener('mousedown', onMouseDown, true);
  document.removeEventListener('selectstart', onSelectStart, true);
  document.removeEventListener('contextmenu', onContextMenu, true);
});
</script>

<template>
  <HomePage />
</template>
