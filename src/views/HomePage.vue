<template>
  <div class="homepage">
    <div class="homepage-header">
      <h1>DeepAhead</h1>
      <p>用最简单的架构，做最有深度的产品</p>
    </div>
    <div class="search-box">
      <span class="search-icon">&#128269;</span>
      <input type="text" v-model="searchQuery" placeholder="搜索本地项目...">
    </div>
    <div class="action-buttons">
      <button class="action-button" @click="emit('navigate', 'ssh')">
        <span class="icon">&#128274;</span><span>SSH连接</span>
      </button>
      <button class="action-button" @click="emit('navigate', 'newProject')">
        <span class="icon">&#128193;</span><span>新建项目</span>
      </button>
      <button class="action-button" @click="emit('navigate', 'openProject')">
        <span class="icon">&#128194;</span><span>打开项目</span>
      </button>
      <button class="action-button" @click="emit('navigate', 'gitClone')">
        <span class="icon">&#11015;</span><span>Git下拉项目</span>
      </button>
    </div>
    <div style="margin-top:2rem;font-size:0.82rem;color:#bbb">
      Tauri 2.x + Rust + Vue 3 · 全部走 DeepSeek Token
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from "vue";

const emit = defineEmits<{ (e: "navigate", page: string): void }>();
const searchQuery = ref("");
</script>

<style scoped>
.homepage { display:flex; flex-direction:column; align-items:center; justify-content:center; width:100%; height:100%; background:#ffffff; }
.homepage-header { text-align:center; margin-bottom:2.5rem; }
.homepage-header h1 { font-size:2.8rem; font-weight:300; color:#1a1a1a; letter-spacing:2px; margin-bottom:0.4rem; }
.homepage-header p { font-size:1rem; color:#999; }
.search-box { width:520px; margin-bottom:2.5rem; position:relative; }
.search-box input {
  width:100%; padding:0.85rem 1.2rem 0.85rem 2.8rem;
  border:1px solid #e0e0e0; border-radius:8px; font-size:0.95rem;
  background:#ffffff; color:#333; outline:none; transition:border-color 0.2s;
  box-sizing:border-box;
}
.search-box input:focus { border-color:#bbb; }
.search-box input::placeholder { color:#bbb; }
.search-icon { position:absolute; left:0.9rem; top:50%; transform:translateY(-50%); color:#ccc; font-size:1rem; pointer-events:none; }
.action-buttons { display:grid; grid-template-columns:repeat(4,1fr); gap:1rem; width:580px; }
.action-button {
  display:flex; flex-direction:column; align-items:center; justify-content:center;
  padding:1.4rem 1rem; background:#ffffff; border:1px solid #e8e8e8;
  border-radius:10px; color:#555; font-size:0.85rem; cursor:pointer;
  transition:all 0.2s; gap:0.6rem;
}
.action-button:hover { border-color:#ccc; box-shadow:0 2px 8px rgba(0,0,0,0.06); }
.action-button .icon { font-size:1.6rem; color:#888; }
</style>
