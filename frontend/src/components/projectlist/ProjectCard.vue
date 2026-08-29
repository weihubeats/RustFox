<script setup lang="ts">
/**
 * ProjectCard：项目卡片。
 *
 * - 彩色渐变头像 / 状态标签（Active/Draft）/ API 数与时间指标；
 * - 更多菜单（重命名 / 复制 / 删除，菜单开合状态由父级统一管理）；
 * - hover 显示 Open 箭头，点击整卡进入项目。
 */
import Icon from '../ui/Icon.vue'
import IconButton from '../ui/IconButton.vue'
import type { Project } from '../../types/foxApi'
import { avatarStyle, initials, timeAgo } from './projectMeta'

defineProps<{
  project: Project
  count: number
  active: boolean
  /** 当前展开更多菜单的卡片 id（父级持有，保证同时只开一个） */
  menuOpen: boolean
  /** 手动排序模式下显示拖拽手柄 */
  draggable: boolean
}>()

const emit = defineEmits<{
  open: []
  'toggle-menu': []
  rename: []
  duplicate: []
  delete: []
}>()
</script>

<template>
  <div class="proj-card" :data-project-id="project.id" @click="emit('open')">
    <span v-if="draggable" class="dnd-handle" title="拖拽排序" @click.stop>
      <svg width="10" height="14" viewBox="0 0 10 14" fill="currentColor" aria-hidden="true">
        <circle cx="2" cy="2" r="1.3" />
        <circle cx="8" cy="2" r="1.3" />
        <circle cx="2" cy="7" r="1.3" />
        <circle cx="8" cy="7" r="1.3" />
        <circle cx="2" cy="12" r="1.3" />
        <circle cx="8" cy="12" r="1.3" />
      </svg>
    </span>
    <span class="proj-avatar" :style="avatarStyle(project.name)">{{ initials(project.name) }}</span>
    <div class="proj-main">
      <div class="proj-title-row">
        <span class="proj-title" v-tooltip-overflow="project.name">{{ project.name }}</span>
        <span class="proj-status" :class="{ active }">
          <span class="status-dot" aria-hidden="true"></span>
          {{ active ? 'Active' : 'Draft' }}
        </span>
      </div>
      <p class="proj-desc" :class="{ empty: !project.description }">
        {{ project.description || '暂无项目描述...' }}
      </p>
      <div class="proj-footer">
        <div class="proj-metrics">
          <span class="metric"><Icon name="plug" :size="12" />{{ count }} APIs</span>
          <span class="metric-sep">·</span>
          <span class="metric"><Icon name="clock" :size="12" />{{ timeAgo(project.updated_at) }}更新</span>
        </div>
        <span class="proj-open">
          进入项目 <Icon name="arrow-up-right" :size="12" />
        </span>
      </div>
    </div>
    <div class="proj-side" data-no-drag>
      <div class="proj-more" @click.stop>
        <IconButton name="more-horizontal" :size="16" title="更多操作" @click="emit('toggle-menu')" />
        <div v-if="menuOpen" class="more-menu" role="menu">
          <button class="menu-item" type="button" @click="emit('rename')">
            <Icon name="pencil" :size="13" /> 重命名
          </button>
          <button class="menu-item" type="button" @click="emit('duplicate')">
            <Icon name="copy" :size="13" /> 复制
          </button>
          <button class="menu-item danger" type="button" @click="emit('delete')">
            <Icon name="trash" :size="13" /> 删除
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
/* ---------- 项目卡片 ---------- */
.proj-card {
  position: relative;
  display: flex;
  gap: 14px;
  padding: 16px;
  /* 网格子项：允许收缩到轨道宽，长描述走 ellipsis 而不是撑爆网格 */
  min-width: 0;
  border-radius: var(--radius-lg);
  /* 半透明深底（#18181b 质感）+ 1px 微光描边，与面板底色拉开层次 */
  border: 1px solid rgba(255, 255, 255, 0.055);
  background:
    linear-gradient(180deg, rgba(255, 255, 255, 0.02), rgba(255, 255, 255, 0) 60%),
    rgba(255, 255, 255, 0.018);
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.25);
  cursor: pointer;
  transition:
    border-color var(--dur) var(--ease),
    background var(--dur) var(--ease),
    transform var(--dur) var(--ease),
    box-shadow var(--dur) var(--ease);
}
.proj-card:hover {
  border-color: rgba(168, 85, 247, 0.5);
  background:
    linear-gradient(180deg, rgba(168, 85, 247, 0.05), rgba(255, 255, 255, 0) 60%),
    rgba(255, 255, 255, 0.028);
  transform: translateY(-2px);
  box-shadow:
    0 14px 34px rgba(0, 0, 0, 0.45),
    0 0 0 1px rgba(168, 85, 247, 0.08),
    0 4px 18px rgba(124, 58, 237, 0.1);
}
html[data-theme='light'] .proj-card {
  border-color: var(--border);
  background: var(--bg-panel);
  box-shadow: var(--shadow);
}
html[data-theme='light'] .proj-card:hover {
  background: var(--bg-panel);
  box-shadow: var(--shadow-lg);
}

.proj-avatar {
  width: 42px;
  height: 42px;
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 12px;
  border: 1px solid rgba(255, 255, 255, 0.12);
  /* 内高光 + 投影：玻璃方块质感，替代纯平色块 */
  box-shadow:
    inset 0 1px 0 rgba(255, 255, 255, 0.18),
    0 2px 8px rgba(0, 0, 0, 0.3);
  font-size: 15px;
  font-weight: 700;
  letter-spacing: 0.02em;
  text-shadow: 0 1px 2px rgba(0, 0, 0, 0.35);
  user-select: none;
}

/* 拖拽手柄：左上角 6 点，hover 浮现，仅手柄可拖 */
.dnd-handle {
  position: absolute;
  top: 12px;
  left: 8px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 16px;
  height: 20px;
  border-radius: 5px;
  color: var(--text-3);
  cursor: grab;
  opacity: 0;
  transition:
    opacity var(--dur) var(--ease),
    color var(--dur) var(--ease),
    background var(--dur) var(--ease);
  user-select: none;
}
.proj-card:hover .dnd-handle {
  opacity: 1;
}
.dnd-handle:hover {
  color: var(--text-1);
  background: var(--bg-hover);
}
.dnd-handle:active {
  cursor: grabbing;
}

.proj-main {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.proj-title-row {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
  /* 给右侧 ⋯ 菜单留出安全距离 */
  padding-right: 6px;
}

.proj-title {
  font-size: 14px;
  font-weight: 700;
  color: var(--text-1);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* 状态 Pill：小圆点 + 弱化文字（Active 绿点 / Draft 中性灰点） */
.proj-status {
  flex-shrink: 0;
  display: inline-flex;
  align-items: center;
  gap: 5px;
  font-size: 10px;
  font-weight: 600;
  padding: 2px 8px;
  border-radius: 999px;
  background: rgba(161, 161, 170, 0.09);
  color: #a1a1aa;
}
.proj-status .status-dot {
  width: 5px;
  height: 5px;
  border-radius: 50%;
  background: currentColor;
  box-shadow: 0 0 4px currentColor;
}
.proj-status.active {
  background: rgba(52, 211, 153, 0.09);
  color: #34d399;
}

.proj-desc {
  margin: 0;
  font-size: 12px;
  color: var(--text-2);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
/* 空描述：更淡的占位文案 */
.proj-desc.empty {
  color: #737373;
}

/* 底栏：分隔线 + 指标行（修复文案重叠：显式行高与间距）+ hover「进入项目 →」 */
.proj-footer {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-top: 12px;
  padding-top: 10px;
  border-top: 1px solid rgba(255, 255, 255, 0.05);
}

.proj-metrics {
  display: flex;
  align-items: center;
  gap: 12px;
  font-size: 11px;
  line-height: 1.5;
  color: #737373;
}

.metric {
  display: inline-flex;
  align-items: center;
  gap: 4px;
}

.metric-sep {
  color: var(--border-strong);
}

/* ---------- 卡片右侧：更多菜单 ---------- */
.proj-side {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  flex-shrink: 0;
}

/* 进入项目 →：hover 浮现于底栏右下角 */
.proj-open {
  margin-left: auto;
  display: inline-flex;
  align-items: center;
  gap: 3px;
  font-size: 11px;
  font-weight: 600;
  color: var(--accent);
  opacity: 0;
  transform: translateX(-4px);
  transition:
    opacity var(--dur) var(--ease),
    transform var(--dur) var(--ease);
}
.proj-card:hover .proj-open {
  opacity: 1;
  transform: translateX(0);
}

.proj-more {
  position: relative;
  flex-shrink: 0;
}

.more-menu {
  position: absolute;
  top: 30px;
  right: 0;
  z-index: 10;
  min-width: 132px;
  padding: 4px;
  border-radius: var(--radius);
  border: 1px solid var(--border-strong);
  background: var(--bg-elevated);
  box-shadow: var(--shadow-lg);
  animation: menu-in 120ms var(--ease);
}

.menu-item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  height: 30px;
  padding: 0 10px;
  border: none;
  border-radius: var(--radius-sm);
  background: none;
  color: var(--text-1);
  font-size: 12.5px;
  font-family: inherit;
  text-align: left;
  cursor: pointer;
  transition: background var(--dur) var(--ease);
}
.menu-item:hover {
  background: var(--bg-hover);
}
.menu-item.danger {
  color: var(--danger);
}
.menu-item.danger:hover {
  background: var(--danger-tint);
}

@keyframes menu-in {
  from {
    opacity: 0;
    transform: translateY(-3px) scale(0.98);
  }
  to {
    opacity: 1;
    transform: translateY(0) scale(1);
  }
}
</style>
