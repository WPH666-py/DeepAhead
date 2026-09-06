<template>
  <div class="sub-page">
    <div class="sub-header">
      <button class="back-btn" @click="emit('navigate', 'home')">&#8592; 返回</button>
      <h2>SSH连接</h2>
    </div>
    <div class="sub-content">
      <div class="sub-form">
        <div class="form-group">
          <label>用户名</label>
          <input v-model="username" placeholder="输入用户名" />
        </div>
        <div class="form-group">
          <label>主机IP</label>
          <input v-model="host" placeholder="输入主机IP地址" />
        </div>
        <div class="form-group">
          <label>密码</label>
          <input v-model="password" type="password" placeholder="输入密码" />
        </div>
        <div class="form-row" style="display:flex;gap:0.6rem">
          <div class="form-group" style="flex:1;margin-bottom:0">
            <label>端口号</label>
            <input v-model.number="port" type="number" placeholder="22" />
          </div>
          <div class="form-group" style="flex:1;margin-bottom:0">
            <label>&nbsp;</label>
            <button class="dir-select-btn" style="width:100%" @click="emit('navigate', 'home')">下载 Trae</button>
          </div>
        </div>
        <div v-if="status" class="status-msg" :class="statusClass">{{ status }}</div>
        <div class="form-actions">
          <button class="btn btn-secondary" @click="emit('navigate', 'home')">取消</button>
          <button class="btn btn-primary" @click="testConnection" :disabled="connecting">
            {{ connecting ? '连接中...' : '连接' }}
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from "vue";
import { tauriAPI } from "../services/tauri-api";

const emit = defineEmits<{ (e: "navigate", page: string): void }>();

const host = ref("");
const port = ref(22);
const username = ref("");
const password = ref("");
const keyPath = ref("");
const connecting = ref(false);
const status = ref("");
const statusClass = ref("");

async function testConnection() {
  connecting.value = true;
  status.value = "";
  try {
    const msg = await tauriAPI.sshTest({
      host: host.value,
      port: port.value,
      username: username.value,
      password: password.value || undefined,
      key_path: keyPath.value || undefined,
    });
    status.value = msg;
    statusClass.value = "success";
  } catch (e: any) {
    status.value = e;
    statusClass.value = "error";
  } finally {
    connecting.value = false;
  }
}
</script>
