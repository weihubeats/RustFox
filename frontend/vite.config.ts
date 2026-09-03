import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import tailwindcss from '@tailwindcss/vite'

export default defineConfig({
  plugins: [vue(), tailwindcss()],
  // Tauri 开发模式固定端口：tauri.conf.json 的 devUrl 指向 5173
  server: {
    port: 5173,
    strictPort: true,
  },
  // Tauri 构建要求使用相对路径（产物经由 tauri://localhost 加载）
  base: './',
  clearScreen: false,
  envPrefix: ['VITE_', 'TAURI_ENV_'],
  build: {
    rollupOptions: {
      output: {
        /**
         * 首屏分包：CodeMirror 全家桶 + chart.js 原来全进主 chunk。
         * vue / codemirror / chart 拆独立 chunk，配合路由懒加载，
         * 首屏只下发框架 + 当前路由代码。
         */
        manualChunks: {
          vendor_vue: ['vue', 'vue-router', 'pinia'],
          vendor_codemirror: [
            '@codemirror/state',
            '@codemirror/view',
            '@codemirror/commands',
            '@codemirror/language',
            '@codemirror/lint',
            '@codemirror/autocomplete',
            '@codemirror/lang-json',
          ],
          vendor_chart: ['chart.js', 'vue-chartjs'],
        },
      },
    },
  },
})
