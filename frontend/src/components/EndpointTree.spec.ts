/**
 * EndpointTree 单测：递归树的「新建子文件夹」链路回归。
 *
 * 背景：行内编辑状态若不跨递归实例共享（或共享键被声明为实例级 Symbol），
 * 点「新建子文件夹」后输入框永远不渲染（状态留在触发菜单的父实例，
 * 输入行渲染条件在 folderId === parentId 的【子】实例上）。
 */
import { describe, expect, it } from 'vitest'
import { mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import EndpointTree from './EndpointTree.vue'
import { useLocaleStore } from '../stores/locale'
import { useWorkspaceStore } from '../stores/workspace'
import { stubScrollIntoView } from '../testUtils/componentTest'
import type { Folder } from '../types/foxApi'

function folder(id: string, name: string, parentId: string | null): Folder {
  const now = '2026-01-01T00:00:00.000Z'
  return {
    id,
    project_id: 'proj-test-1',
    parent_id: parentId,
    name,
    sort_order: 0,
    created_at: now,
    updated_at: now,
  }
}

async function mountTree() {
  setActivePinia(createPinia())
  // 文案断言锁定中文（jsdom 默认语言为英文，跟随系统会解析出英文）
  useLocaleStore().setMode('zh')
  const store = useWorkspaceStore()
  store.project = {
    id: 'proj-test-1',
    name: 'P',
    description: '',
    variables: {},
    created_at: '2026-01-01T00:00:00.000Z',
    updated_at: '2026-01-01T00:00:00.000Z',
  }
  store.folders = [folder('f-root', '宠物管理', null), folder('f-sub', '子目录', 'f-root')]
  store.endpoints = []
  const wrapper = mount(EndpointTree, {
    props: { folderId: null, search: '' },
    global: { plugins: [] },
    attachTo: document.body,
  })
  return { wrapper, store }
}

describe('EndpointTree：新建子文件夹', () => {
  it('点击「新建子文件夹」后，目标文件夹子树内出现行内输入框', async () => {
    const restore = stubScrollIntoView()
    try {
      const { wrapper } = await mountTree()

      // 根级文件夹「宠物管理」行上的 ⋯ 按钮
      const row = wrapper.find('[data-dnd-kind="folder"][data-dnd-id="f-root"]')
      expect(row.exists()).toBe(true)
      await row.find('button').trigger('click')

      // Menu teleport 到 body：从 document 中找到「新建子文件夹」菜单项并点击
      const item = [...document.querySelectorAll('button')].find((b) =>
        b.textContent?.includes('新建子文件夹'),
      )
      expect(item, '菜单项应已渲染').toBeDefined()
      item!.dispatchEvent(new MouseEvent('click', { bubbles: true }))
      await new Promise((r) => setTimeout(r, 0))

      // 输入行渲染在【子实例】（folderId === f-root）的子树里：
      // 共享状态生效的直接证据——父实例本地状态永远到不了这里
      const childrenOfFolder = wrapper.find(
        '[data-dnd-tree-root="f-root"], .tree-children [data-dnd-tree-root="f-root"]',
      )
      const input = childrenOfFolder.find('input.tree-input')
      expect(input.exists(), '子文件夹行内输入框应出现').toBe(true)
      expect(input.attributes('placeholder')).toBe('文件夹名称')
    } finally {
      restore()
    }
  })
})
