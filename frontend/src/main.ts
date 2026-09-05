/**
 * 应用入口：装配 Pinia（状态）+ Router（视图路由）。
 * 全局 Toast / Progress 由 App.vue 挂载一次，勿在视图内重复实例化。
 */
import { createApp } from 'vue'
import { createPinia } from 'pinia'
import App from './App.vue'
import router from './router'
import { useThemeStore } from './stores/theme'
import { useLocaleStore } from './stores/locale'
import '@fontsource/geist-sans/400.css'
import '@fontsource/geist-sans/500.css'
import '@fontsource/geist-sans/600.css'
import '@fontsource/geist-sans/700.css'
import '@fontsource/jetbrains-mono/400.css'
import '@fontsource/jetbrains-mono/500.css'
import '@fontsource/jetbrains-mono/700.css'
import './style.css'
import tooltipOverflow from './directives/tooltipOverflow'
import focusEnd from './directives/focusEnd'

// macOS 无边框标题栏（Overlay）：标记平台，供全局 CSS 为顶部栏预留交通灯空间。
if (navigator.userAgent.includes('Mac')) {
  document.documentElement.setAttribute('data-platform', 'macos')
}

const pinia = createPinia()

// 防闪烁（FOUC）：在挂载前同步读取持久化主题与语言并写入 <html>。
useThemeStore(pinia).init()
useLocaleStore(pinia).init()

createApp(App).use(pinia).use(router).directive('tooltip-overflow', tooltipOverflow).directive('focus-end', focusEnd).mount('#app')
