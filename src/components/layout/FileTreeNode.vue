<template>
  <div>
    <div
      class="file-item"
      :class="{ dir: entry.is_dir, file: !entry.is_dir, expanded: entry.is_dir && expanded, selected: store.selectedFile === entry.path, open: isOpen }"
      :style="{ paddingLeft: depth * 16 + 8 + 'px' }"
      @click="handleClick"
      @contextmenu="onContextMenu"
    >
      <span v-if="entry.is_dir" class="expand-icon">{{ expanded ? '▼' : '▶' }}</span>
      <span v-else class="expand-icon" style="visibility:hidden">▶</span>
      <span class="item-icon">{{ entry.is_dir ? (expanded ? '📂' : '📁') : getFileIcon(entry.name) }}</span>
      <span class="item-name">{{ entry.name }}</span>
    </div>
    <template v-if="entry.is_dir && expanded && entry.children">
      <FileTreeNode
        v-for="child in entry.children"
        :key="child.path"
        :entry="child"
        :depth="depth + 1"
        :open-tabs="openTabs"
        @context-menu="(e: MouseEvent, entry: FileEntry) => emit('contextMenu', e, entry)"
        @open="(path: string) => emit('open', path)"
        @toggle="(entry: FileEntry) => emit('toggle', entry)"
      />
    </template>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from "vue";
import { useAppStore } from "../../stores/app";
import type { FileEntry } from "../../services/tauri-api";

const props = defineProps<{
  entry: FileEntry;
  depth: number;
  openTabs?: string[];
}>();

const isOpen = computed(() => {
  return props.openTabs?.includes(props.entry.path) ?? false;
});

const emit = defineEmits<{
  (e: "contextMenu", ev: MouseEvent, entry: FileEntry): void;
  (e: "open", path: string): void;
  (e: "toggle", entry: FileEntry): void;
}>();

const store = useAppStore();
const expanded = ref(props.depth < 1);

function handleClick(e: MouseEvent) {
  e.stopPropagation();
  store.selectedFile = props.entry.path;
  if (props.entry.is_dir) {
    expanded.value = !expanded.value;
    emit("toggle", props.entry);
  } else {
    emit("open", props.entry.path);
  }
}

function onContextMenu(e: MouseEvent) {
  e.preventDefault();
  e.stopPropagation();
  store.selectedFile = props.entry.path;
  emit("contextMenu", e, props.entry);
}

function getFileIcon(name: string) {
  const ext = name.split(".").pop()?.toLowerCase() || "";
  const iconMap: Record<string, string> = {
    js: "🟨", ts: "🟦", jsx: "⚛", tsx: "⚛",
    py: "🐍", java: "☕", go: "🐹", rs: "🦀",
    cpp: "🔵", c: "🔵", h: "🟣", hpp: "🟣",
    html: "🌐", htm: "🌐", css: "🎨", scss: "🎨",
    json: "🗂️", md: "📝", txt: "📄",
    png: "🖼️", jpg: "🖼️", jpeg: "🖼️", gif: "🖼️", webp: "🖼️", svg: "🖼️",
    pdf: "📕", xlsx: "📊", xls: "📊", csv: "📊",
    doc: "📘", docx: "📘", ppt: "📙", pptx: "📙",
    zip: "🗜️", rar: "🗜️", "7z": "🗜️",
    sh: "🐚", bat: "🦇", ps1: "🦕",
  };
  return iconMap[ext] || "📄";
}
</script>
