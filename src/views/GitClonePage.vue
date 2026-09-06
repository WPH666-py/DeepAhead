<template>
  <div class="sub-page">
    <div class="sub-header">
      <button class="back-btn" @click="emit('navigate', 'home')">&#8592; 返回</button>
      <h2>Git下拉项目</h2>
    </div>
    <div class="sub-content">
      <div class="sub-form">
        <div class="form-group">
          <label>GitHub仓库链接</label>
          <input v-model="repoUrl" placeholder="https://github.com/user/repo.git" />
        </div>
        <div class="form-group">
          <label>Git代理网址（可选）</label>
          <input v-model="proxyUrl" placeholder="https://proxy.example.com" />
        </div>
        <div class="form-group">
          <label>目标目录</label>
          <div class="dir-select-row">
            <div class="form-group" style="flex:1;margin-bottom:0">
              <input v-model="targetPath" placeholder="选择目标目录" readonly />
            </div>
            <button class="dir-select-btn" @click="browsePath">浏览</button>
          </div>
        </div>
        <div v-if="status" class="status-msg" :class="statusClass">{{ status }}</div>
        <div class="form-actions">
          <button class="btn btn-secondary" @click="emit('navigate', 'home')">取消</button>
          <button class="btn btn-primary" @click="handleClone" :disabled="!repoUrl || !targetPath || cloning">
            {{ cloning ? '下拉中...' : '下拉' }}
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

const emit = defineEmits<{ (e: "navigate", page: string): void }>();

const repoUrl = ref("");
const proxyUrl = ref("");
const targetPath = ref("");
const cloning = ref(false);
const status = ref("");
const statusClass = ref("");

async function browsePath() {
  const selected = await open({ directory: true, multiple: false, title: "选择目标目录" });
  if (selected) targetPath.value = selected as string;
}

async function handleClone() {
  if (!repoUrl.value || !targetPath.value) return;
  cloning.value = true;
  status.value = "正在克隆仓库...";
  statusClass.value = "info";
  try {
    const result = await tauriAPI.gitClone(repoUrl.value, targetPath.value, proxyUrl.value || undefined);
    status.value = result;
    statusClass.value = "success";
    setTimeout(() => emit("navigate", "editor"), 1000);
  } catch (e: any) {
    status.value = e;
    statusClass.value = "error";
  } finally {
    cloning.value = false;
  }
}
</script>
