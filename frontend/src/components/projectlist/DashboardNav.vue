<script setup lang="ts">
/**
 * DashboardNav：仪表板左侧导航。
 *
 * - 仅保留已实现入口：仪表板 / API 项目（主页面）/ 设置；
 * - 集合、API 文档等未实现模块暂不展示；设置项 emit('settings') 打开设置弹窗。
 */
import { useRouter } from 'vue-router'
import Icon from '../ui/Icon.vue'

const router = useRouter()

const emit = defineEmits<{ settings: [] }>()

const NAV_ITEMS = [
  { key: 'dashboard', label: '仪表板', icon: 'gauge' as const, route: '/projects' },
  { key: 'projects', label: 'API 项目', icon: 'folder' as const, route: '/projects' },
  { key: 'settings', label: '设置', icon: 'settings' as const, route: '', settings: true },
]

function navActive(item: (typeof NAV_ITEMS)[number]): boolean {
  return item.route === '/projects'
}

function onNav(item: (typeof NAV_ITEMS)[number]): void {
  if (item.settings) {
    emit('settings')
    return
  }
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
  height: 34px;
  padding: 0 10px 0 8px;
  border: none;
  /* 左侧 2px 高亮条占位（active 时着色），避免选中态布局跳动 */
  border-left: 2px solid transparent;
  border-radius: var(--radius);
  background: none;
  color: var(--text-2);
  font-size: 13px;
  font-family: inherit;
  text-align: left;
  cursor: pointer;
  transition:
    background var(--dur) var(--ease),
    color var(--dur) var(--ease),
    border-color var(--dur) var(--ease);
}
.nav-item:hover {
  background: var(--bg-hover);
  color: var(--text-1);
}
.nav-item:active {
  background: var(--bg-active);
}
/* 选中态：半透明紫底 + 紫色文字 + 左缘主题色条 */
.nav-item.active {
  background: rgba(124, 58, 237, 0.1);
  color: #a78bfa;
  border-left-color: #a855f7;
  font-weight: 500;
}

.nav-label {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

</style>
