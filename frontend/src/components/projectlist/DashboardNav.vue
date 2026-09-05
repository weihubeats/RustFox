<script setup lang="ts">
/**
 * DashboardNav：仪表板左侧导航。
 *
 * - 仅保留已实现入口：仪表板 / API 项目（主页面）；
 * - 设置入口在顶栏右上角（齿轮按钮），导航不再重复；
 * - 集合、API 文档等未实现模块暂不展示。
 */
import { useRouter } from 'vue-router'
import Icon from '../ui/Icon.vue'

const router = useRouter()

const NAV_ITEMS = [
  { key: 'dashboard', label: '仪表板', icon: 'gauge' as const, route: '/projects' },
  { key: 'projects', label: 'API 项目', icon: 'folder' as const, route: '/projects' },
]

function navActive(item: (typeof NAV_ITEMS)[number]): boolean {
  return item.route === '/projects'
}

function onNav(item: (typeof NAV_ITEMS)[number]): void {
  router.push(item.route)
}
</script>

<template>
  <nav class="dash-nav" aria-label="主导航">
    <button
      v-for="item in NAV_ITEMS"
      :key="item.key"
      class="nav-item"
      :class="{ active: navActive(item) }"
      type="button"
      @click="onNav(item)"
    >
      <Icon :name="item.icon" :size="15" />
      <span class="nav-label">{{ item.label }}</span>
    </button>
  </nav>
</template>

<style scoped>
.dash-nav {
  width: 200px;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 16px 10px;
  border-right: 1px solid var(--border);
  background: var(--bg-panel);
  overflow-y: auto;
}

.nav-item {
  display: flex;
  align-items: center;
  gap: 10px;
  height: 36px;
  padding: 0 12px;
  border: none;
  border-radius: 10px;
  background: none;
  color: var(--text-2);
  font-size: 13px;
  font-family: inherit;
  text-align: left;
  cursor: pointer;
  transition:
    background var(--dur) var(--ease),
    color var(--dur) var(--ease),
    box-shadow var(--dur) var(--ease);
}
.nav-item:hover {
  background: var(--bg-hover);
  color: var(--text-1);
}
.nav-item:active {
  background: var(--bg-active);
}
/* 选中态：Obsidian 全宽紫 pill（渐变 + 光晕 + 白字） */
.nav-item.active {
  background: linear-gradient(135deg, #7e57ff, #6e46ff);
  color: #fff;
  font-weight: 600;
  box-shadow:
    inset 0 1px 0 rgba(255, 255, 255, 0.22),
    0 4px 14px rgba(126, 87, 255, 0.35);
}

.nav-label {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

</style>
