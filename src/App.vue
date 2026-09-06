<template>
  <div id="DeepAhead" class="app-container">
    <HomePage v-if="currentPage === 'home'" @navigate="navigateTo" />
    <NewProjectPage v-else-if="currentPage === 'newProject' || currentPage === 'new-project'" @navigate="navigateTo" />
    <OpenProjectPage v-else-if="currentPage === 'openProject' || currentPage === 'open-project'" @navigate="navigateTo" />
    <SSHPage v-else-if="currentPage === 'ssh'" @navigate="navigateTo" />
    <GitClonePage v-else-if="currentPage === 'gitClone'" @navigate="navigateTo" />
    <EditorPage v-else-if="currentPage === 'editor'" @navigate="navigateTo" />
  </div>
</template>

<script setup lang="ts">
import { ref } from "vue";
import HomePage from "./views/HomePage.vue";
import NewProjectPage from "./views/NewProjectPage.vue";
import OpenProjectPage from "./views/OpenProjectPage.vue";
import SSHPage from "./views/SSHPage.vue";
import GitClonePage from "./views/GitClonePage.vue";
import EditorPage from "./views/EditorPage.vue";

const currentPage = ref<string>("home");

function navigateTo(page: string) {
  currentPage.value = page;
}
</script>

<style scoped>
.app-container { width: 100vw; height: 100vh; overflow: hidden; }
</style>
