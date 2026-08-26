<script setup lang="ts">
/**
 * ProjectTabs：顶栏多项目标签条（工作区 / 项目首页共用）。
 * - 点击标签：切换项目并进入工作区（首页点击同样跳转）；
 * - × 关闭标签（快照丢弃；关闭当前项目时 store 自动切相邻标签）；
 * - ⋯ 菜单：未打开的项目（点击打开并切换）+ 新建项目（emit 给宿主）+ 项目列表。
 */
import { ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useWorkspaceStore } from '../stores/workspace'
import { useFoxApi } from '../composables/useFoxApi'
import { useToast } from '../composables/useToast'
import Icon from './ui/Icon.vue'
import Menu, { type MenuItem } from './ui/Menu.vue'

const emit = defineEmits<{ 'new-project': [] }>()

const store = useWorkspaceStore()
const api = useFoxApi()
const toast = useToast()
const router = useRouter()
const route = useRoute()

const menuRef = ref<InstanceType<typeof Menu> | null>(null)
const moreBtn = ref<HTMLButtonElement | null>(null)

async function openMore(): Promise<void> {
  if (!moreBtn.value) return
  let items: MenuItem[] = []
  try {
    const openIds = new Set(store.openProjects.map((t) => t.id))
    items = (await api.getProjects())
      .filter((p) => !openIds.has(p.id))
      .map((p) => ({
        key: `switch-project:${p.id}`,
        label: p.name,
        icon: 'folder' as const,
      }))
    if (items.length) items[0] = { ...items[0], dividerBefore: true }
  } catch {
    toast.error('项目列表加载失败')
  }
  items.push(
    { key: 'new-project', label: '新建项目', icon: 'plus', iconAccent: true, dividerBefore: true },
    { key: 'go-projects', label: '项目列表', icon: 'folder' },
  )
  menuRef.value?.openAt(moreBtn.value, items, 'left')
}

function onSelect(item: MenuItem): void {
  if (item.key.startsWith('switch-project:')) void switchTo(item.key.slice('switch-project:'.length))
  else if (item.key === 'new-project') emit('new-project')
  else if (item.key === 'go-projects') router.push('/projects')
}

async function switchTo(projectId: string): Promise<void> {
  try {
    await store.switchProject(projectId)
    // 标签高亮 + 内容切换已是充分反馈，成功不弹 toast（避免频繁切换被打扰）
    if (route.path !== '/workspace') router.push('/workspace')
  } catch (err) {
    toast.error('切换项目失败', { message: err instanceof Error ? err.message : String(err) })
  }
}

function onTabClick(projectId: string): void {
  void switchTo(projectId)
}

function onClose(projectId: string): void {
  store.closeProjectTab(projectId)
}
</script>

<template>
  <div class="ptabs">
    <div class="proj-tabs">
      <div
        v-for="tab in store.openProjects"
        :key="tab.id"
        class="proj-tab"
        :class="{ active: tab.id === store.project?.id }"
        :title="tab.name"
        @click="onTabClick(tab.id)"
      >
        <span class="pt-name">{{ tab.name }}</span>
        <button
          class="pt-close"
          type="button"
          :title="`关闭「${tab.name}」标签`"
          @click.stop="onClose(tab.id)"
        >
          <Icon name="x" :size="11" />
        </button>
      </div>
    </div>
    <button ref="moreBtn" class="proj-more" type="button" title="更多项目" @click="openMore">
      <Icon name="more-horizontal" :size="14" />
    </button>
  </div>
  <Menu ref="menuRef" @select="onSelect" />
</template>

<style scoped>
.ptabs {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
  flex: 1 1 auto;
}

.proj-tabs {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
  overflow-x: auto;
  scrollbar-width: none;
}
.proj-tabs::-webkit-scrollbar {
  display: none;
}

.proj-tab {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  max-width: 180px;
  padding: 3px 8px 3px 12px;
  border: 1px solid transparent;
  border-radius: var(--radius);
  font-size: 12.5px;
  color: var(--text-2);
  cursor: pointer;
  user-select: none;
  transition:
    background var(--dur) var(--ease),
    color var(--dur) var(--ease),
    border-color var(--dur) var(--ease);
}
.proj-tab:hover {
  background: var(--bg-hover);
  color: var(--text-1);
}
.proj-tab.active {
  background: var(--bg-elevated);
  border-color: var(--border-strong);
  color: var(--text-1);
  font-weight: 600;
}

.pt-name {
  min-width: 0;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.pt-close {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 16px;
  height: 16px;
  padding: 0;
  border: none;
  border-radius: 4px;
  background: none;
  color: var(--text-3);
  cursor: pointer;
  flex-shrink: 0;
}
.pt-close:hover {
  background: var(--bg-active);
  color: var(--text-1);
}
.proj-tab:not(:hover) .pt-close {
  opacity: 0;
}
.proj-tab.active .pt-close {
  opacity: 1;
}

.proj-more {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  padding: 0;
  border: none;
  border-radius: var(--radius);
  background: none;
  color: var(--text-2);
  cursor: pointer;
  flex-shrink: 0;
}
.proj-more:hover {
  background: var(--bg-hover);
  color: var(--text-1);
}
</style>
