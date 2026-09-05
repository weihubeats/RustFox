<script setup lang="ts">
/**
 * TabBar：打开中的接口标签页。
 * - 每个标签：方法标签（GET 绿 / POST 黄…）+ 截断的接口名；宽 120–200px；
 * - 激活态 = text-1 + 底部主题色下划线；未保存草稿在标题旁显示小圆点，
 *   hover 时圆点被 ✕ 替换（两者不同时出现，避免挤占）；
 * - 关闭按钮 hover 才出现；脏标签关闭走 Popconfirm。
 */
import { computed, nextTick, ref, watch } from 'vue'
import { useWorkspaceStore } from '../stores/workspace'
import { useToast } from '../composables/useToast'
import { useLocaleStore } from '../stores/locale'
import { methodTone } from '../utils/methodTone'
import Icon from './ui/Icon.vue'
import IconButton from './ui/IconButton.vue'
import Menu, { type MenuItem } from './ui/Menu.vue'
import Popconfirm from './ui/Popconfirm.vue'
import Tooltip from './ui/Tooltip.vue'

const store = useWorkspaceStore()
const toast = useToast()
const locale = useLocaleStore()
const t = locale.t

const emit = defineEmits<{
  'import-curl': []
  'import-openapi': []
}>()

function close(id: string): void {
  store.closeTab(id)
}

/** 中键直接关闭（对标浏览器标签页；mousedown 阶段拦截以抑制中键自动滚动）。 */
function onTabMouseDown(event: MouseEvent, id: string): void {
  if (event.button === 1) {
    event.preventDefault()
    close(id)
  }
}

// ---------- 激活标签滚动到可视区 ----------
const barEl = ref<HTMLElement | null>(null)

watch(
  () => store.activeTabId,
  async () => {
    await nextTick()
    barEl.value
      ?.querySelector('.tab.active')
      ?.scrollIntoView({ block: 'nearest', inline: 'nearest' })
  },
)

/**
 * 标签行数据一次求值：原来模板每标签调 methodOf/titleOf/isDirty 3-4 次
 *（每次各自查 drafts/endpoints），此处单遍组装供模板复用。
 */
const tabs = computed(() =>
  store.openTabs.map((id) => ({
    id,
    method: store.draftOf(id)?.method ?? 'GET',
    title: store.titleOf(id),
    dirty: store.isDirty(id),
  })),
)

// ---------- 「+」快捷新建：主区 = 空 HTTP 请求，箭头 = 新建类型菜单 ----------
const addMenu = ref<InstanceType<typeof Menu> | null>(null)
const addArrowEl = ref<HTMLButtonElement | null>(null)

const ADD_MENU_ITEMS = computed<MenuItem[]>(() => [
  { key: 'endpoint', label: t('tabbar.newRequest'), icon: 'zap', shortcut: '⌘N' },
  { key: 'curl', label: t('tabbar.importCurl'), icon: 'terminal', dividerBefore: true },
  { key: 'openapi', label: t('tabbar.importOpenapi'), icon: 'download' },
  { key: 'folder', label: t('tabbar.newFolderMenu'), icon: 'folder-plus', dividerBefore: true },
])

function openAddMenu(): void {
  if (addArrowEl.value) addMenu.value?.openAt(addArrowEl.value, ADD_MENU_ITEMS.value)
}

function onAddMenuSelect(item: MenuItem): void {
  if (item.key === 'endpoint') {
    store.openNewEndpoint(null)
  } else if (item.key === 'curl') {
    emit('import-curl')
  } else if (item.key === 'openapi') {
    emit('import-openapi')
  } else if (item.key === 'folder') {
    void createFolder()
  }
}

async function createFolder(): Promise<void> {
  const now = new Date().toISOString()
  try {
    await store.saveFolder({
      id: crypto.randomUUID(),
      project_id: store.project?.id ?? '',
      parent_id: null,
      name: t('tabbar.newFolder'),
      sort_order: 0,
      created_at: now,
      updated_at: now,
    })
    toast.success(t('tabbar.folderCreated'))
  } catch (err) {
    toast.error(t('tabbar.folderCreateFail'), {
      message: err instanceof Error ? err.message : String(err),
    })
  }
}
</script>

<template>
  <div ref="barEl" class="tab-bar">
    <div
      v-for="tab in tabs"
      :key="tab.id"
      class="tab"
      :class="{ active: store.activeTabId === tab.id }"
      @click="store.activeTabId = tab.id"
      @mousedown="onTabMouseDown($event, tab.id)"
    >
      <span class="method-tag" :class="methodTone(tab.method)">{{ tab.method }}</span>
      <span class="tab-title" v-tooltip-overflow="tab.title">{{ tab.title }}</span>
      <span v-if="tab.dirty" class="tab-dirty" :title="t('tabbar.unsaved')"><Icon name="dot" :size="7" /></span>
      <Popconfirm
        v-if="tab.dirty"
        :title="t('tabbar.closeConfirm')"
        @confirm="close(tab.id)"
      >
        <IconButton class="tab-close" name="x" :size="12" :title="t('common.close')" />
      </Popconfirm>
      <IconButton v-else class="tab-close" name="x" :size="12" :title="t('common.close')" @click.stop="close(tab.id)" />
    </div>
    <Tooltip :content="t('tabbar.newRequestHint')">
      <div class="tab-add-group">
        <button
          class="tab-add tab-add-main"
          type="button"
          :aria-label="t('tabbar.newRequest')"
          @click="store.openNewEndpoint(null)"
        >
          <Icon name="plus" :size="15" />
        </button>
        <span class="tab-add-sep" aria-hidden="true"></span>
        <button
          ref="addArrowEl"
          class="tab-add tab-add-arrow"
          type="button"
          :aria-label="t('tabbar.newTypeMenu')"
          :title="t('tabbar.newTypeMenu')"
          @click="openAddMenu"
        >
          <Icon name="chevron-down" :size="12" />
        </button>
      </div>
    </Tooltip>
    <Menu ref="addMenu" @select="onAddMenuSelect" />
  </div>
</template>

<style scoped>
.tab-bar {
  display: flex;
  gap: 2px;
  padding: 6px 8px 0;
  overflow-x: auto;
  overflow-y: hidden;
  border-bottom: 1px solid var(--border);
  background: var(--bg-panel);
  flex-shrink: 0;
}

.tab {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 32px;
  min-width: 120px;
  max-width: 200px;
  padding: 0 4px 0 10px;
  border-radius: var(--radius) var(--radius) 0 0;
  position: relative;
  font-size: 12.5px;
  color: var(--text-2);
  cursor: pointer;
  user-select: none;
  white-space: nowrap;
  transition:
    color var(--dur) var(--ease),
    background var(--dur) var(--ease);
}
.tab:hover {
  background: var(--bg-hover);
  color: var(--text-1);
}
.tab.active {
  color: var(--text-1);
  background: var(--bg-app);
}
.tab.active::after {
  content: '';
  position: absolute;
  left: 10px;
  right: 10px;
  bottom: 0;
  height: 2px;
  border-radius: 1px;
  background: var(--accent);
  box-shadow: 0 0 8px var(--accent);
}

/* 方法标签：胶囊布局，颜色走共享 methodTone（utils/methodTone.ts） */
.method-tag {
  flex-shrink: 0;
  font-family: var(--font-mono);
  font-size: 10.5px;
  font-weight: 700;
  line-height: 1;
  padding: 3px 6px;
  border-radius: 999px;
}

.tab-title {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* 未保存圆点：常态显示，hover 时被 ✕ 顶替 */
.tab-dirty {
  display: inline-flex;
  color: var(--warning);
  flex-shrink: 0;
  transition: opacity var(--dur) var(--ease);
}
.tab:hover .tab-dirty {
  opacity: 0;
}

.tab-close {
  width: 20px;
  height: 20px;
  opacity: 0;
  flex-shrink: 0;
  transition:
    opacity var(--dur) var(--ease),
    background var(--dur) var(--ease);
}
.tab:hover .tab-close {
  opacity: 1;
}
.tab-close:hover {
  background: var(--danger-tint);
  color: var(--danger);
}

/* ---- 快捷新建「+」：主区 + 箭头下拉 组合按钮 ---- */
.tab-add-group {
  display: inline-flex;
  align-items: stretch;
  align-self: center;
  flex-shrink: 0;
  height: 28px;
  margin: 0 2px 2px 4px;
  border: 1px solid transparent;
  border-radius: 6px;
  overflow: hidden;
  transition:
    border-color var(--dur) var(--ease),
    background var(--dur) var(--ease);
}
.tab-add-group:hover {
  border-color: var(--border-strong);
  background: var(--bg-active);
}
.tab-add-group:focus-within {
  outline: 2px solid var(--accent);
  outline-offset: 1px;
}

.tab-add {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border: none;
  background: transparent;
  color: var(--accent);
  cursor: pointer;
  padding: 0;
  transition:
    background var(--dur) var(--ease),
    transform var(--dur) var(--ease);
}
.tab-add-main {
  width: 26px;
}
.tab-add-arrow {
  width: 16px;
  color: var(--text-2);
}
.tab-add-arrow:hover {
  color: var(--text-1);
}
.tab-add:active {
  transform: scale(0.92);
}
.tab-add-sep {
  width: 1px;
  background: var(--border);
}
</style>
