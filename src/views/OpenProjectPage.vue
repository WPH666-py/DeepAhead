<template>
  <div class="sub-page">
    <div class="sub-header">
      <button class="back-btn" @click="emit('navigate', 'home')">&#8592; 返回</button>
      <h2>打开项目</h2>
    </div>
    <div class="sub-content">
      <div class="sub-form">
        <div class="form-group">
          <label>项目目录</label>
          <div class="dir-select-row">
            <div class="form-group" style="flex:1;margin-bottom:0">
              <input v-model="projectPath" placeholder="选择要打开的文件目录" readonly />
            </div>
            <button class="dir-select-btn" @click="browsePath">浏览</button>
          </div>
        </div>
        <div v-if="status" class="status-msg" :class="statusClass">{{ status }}</div>
        <div class="form-actions">
          <button class="btn btn-secondary" @click="emit('navigate', 'home')">取消</button>
          <button class="btn btn-primary" @click="handleOpen" :disabled="!projectPath || opening">
            {{ opening ? '打开中...' : '打开' }}
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { tauriAPI } from "../services/tauri-api";
import { useAppStore } from "../stores/app";

const emit = defineEmits<{ (e: "navigate", page: string): void }>();
const store = useAppStore();

const projectPath = ref("");
const opening = ref(false);
const status = ref("");
const statusClass = ref("");

async function browsePath() {
  const selected = await open({ directory: true, multiple: false, title: "选择项目目录" });
  if (selected) projectPath.value = selected as string;
}

async function handleOpen() {
  if (!projectPath.value) return;
  opening.value = true;
  status.value = "";
  try {
    await tauriAPI.openProject(projectPath.value);
    store.setProject(projectPath.value);
    emit("navigate", "editor");
  } catch (e: any) {
    status.value = e;
    statusClass.value = "error";
  } finally {
    opening.value = false;
  }
}
</script>
