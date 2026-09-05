/**
 * useAutoUpdate 单测：启动延迟 / 轮询 / 节流 / 同版本去重 / 失败静默。
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { Update } from '@tauri-apps/plugin-updater'
import { startAutoUpdate, takePendingUpdate } from './useAutoUpdate'
import {
  clearSkippedUpdateVersion,
  skipUpdateVersion,
  skippedUpdateVersion,
} from './useAutoUpdate'

function fakeUpdate(version: string): Update {
  return { available: true, version, body: 'notes', close: vi.fn() } as unknown as Update
}

function fakeCurrent(version: string): Update {
  return { available: false, version, close: vi.fn() } as unknown as Update
}

beforeEach(() => {
  vi.useFakeTimers()
  localStorage.clear()
  // 标记为 Tauri 环境，否则 startAutoUpdate 直接返回空操作
  ;(window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {}
})

afterEach(() => {
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__
  // 排空模块级暂存，避免测试间污染
  takePendingUpdate()?.close()
  vi.useRealTimers()
  vi.restoreAllMocks()
})

describe('useAutoUpdate', () => {
  it('非 Tauri 环境直接返回空操作，不检查', async () => {
    delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__
    const checkFn = vi.fn(async () => fakeUpdate('9.9.9'))
    const stop = startAutoUpdate({ checkFn, startupDelayMs: 1000 })
    await vi.advanceTimersByTimeAsync(5000)
    expect(checkFn).not.toHaveBeenCalled()
    stop()
  })

  it('启动延迟后检查一次，发现新版回调并暂存', async () => {
    const checkFn = vi.fn(async () => fakeUpdate('1.2.0'))
    const onUpdateAvailable = vi.fn()
    const stop = startAutoUpdate({ checkFn, startupDelayMs: 8000, onUpdateAvailable })
    await vi.advanceTimersByTimeAsync(7999)
    expect(checkFn).not.toHaveBeenCalled()
    await vi.advanceTimersByTimeAsync(1)
    expect(checkFn).toHaveBeenCalledTimes(1)
    expect(onUpdateAvailable).toHaveBeenCalledWith({ version: '1.2.0' })
    expect(takePendingUpdate()?.version).toBe('1.2.0')
    expect(takePendingUpdate()).toBeNull()
    stop()
  })

  it('无更新时不回调、不暂存', async () => {
    const checkFn = vi.fn(async () => fakeCurrent('0.0.17'))
    const onUpdateAvailable = vi.fn()
    const stop = startAutoUpdate({ checkFn, startupDelayMs: 100, onUpdateAvailable })
    await vi.advanceTimersByTimeAsync(200)
    expect(onUpdateAvailable).not.toHaveBeenCalled()
    expect(takePendingUpdate()).toBeNull()
    stop()
  })

  it('同一版本只提醒一次，6 小时轮询会再次检查', async () => {
    const checkFn = vi.fn(async () => fakeUpdate('1.2.0'))
    const onUpdateAvailable = vi.fn()
    const stop = startAutoUpdate({
      checkFn,
      startupDelayMs: 100,
      pollIntervalMs: 6 * 60 * 60 * 1000,
      onUpdateAvailable,
    })
    await vi.advanceTimersByTimeAsync(200)
    expect(onUpdateAvailable).toHaveBeenCalledTimes(1)
    // 轮询触发：检查执行，但同版本不再回调（旧暂存被关闭释放）
    await vi.advanceTimersByTimeAsync(6 * 60 * 60 * 1000)
    expect(checkFn).toHaveBeenCalledTimes(2)
    expect(onUpdateAvailable).toHaveBeenCalledTimes(1)
    stop()
  })

  it('距上次检查不足间隔则跳过（多窗口/重启不重复打扰）', async () => {
    localStorage.setItem('rustfox:update:last-check', String(Date.now()))
    const checkFn = vi.fn(async () => fakeUpdate('1.2.0'))
    const stop = startAutoUpdate({ checkFn, startupDelayMs: 100 })
    await vi.advanceTimersByTimeAsync(200)
    expect(checkFn).not.toHaveBeenCalled()
    stop()
  })

  it('检查失败静默忽略，不抛错', async () => {
    const checkFn = vi.fn(async () => {
      throw new Error('offline')
    })
    const onUpdateAvailable = vi.fn()
    const stop = startAutoUpdate({ checkFn, startupDelayMs: 100, onUpdateAvailable })
    await vi.advanceTimersByTimeAsync(200)
    expect(onUpdateAvailable).not.toHaveBeenCalled()
    stop()
  })

  it('停止后不再检查', async () => {
    const checkFn = vi.fn(async () => fakeUpdate('1.2.0'))
    const stop = startAutoUpdate({ checkFn, startupDelayMs: 100, pollIntervalMs: 1000 })
    stop()
    await vi.advanceTimersByTimeAsync(5000)
    expect(checkFn).not.toHaveBeenCalled()
  })

  it('跳过的版本不再提醒，取消跳过后恢复提醒', async () => {
    skipUpdateVersion('1.2.0')
    expect(skippedUpdateVersion()).toBe('1.2.0')
    const checkFn = vi.fn(async () => fakeUpdate('1.2.0'))
    const onUpdateAvailable = vi.fn()
    const stop = startAutoUpdate({ checkFn, startupDelayMs: 100, onUpdateAvailable })
    await vi.advanceTimersByTimeAsync(200)
    expect(checkFn).toHaveBeenCalledTimes(1)
    expect(onUpdateAvailable).not.toHaveBeenCalled()
    expect(takePendingUpdate()).toBeNull()
    stop()

    // 取消跳过 + 清除节流后，下次检查重新提醒
    clearSkippedUpdateVersion()
    expect(skippedUpdateVersion()).toBeNull()
    localStorage.removeItem('rustfox:update:last-check')
    const stop2 = startAutoUpdate({ checkFn, startupDelayMs: 100, onUpdateAvailable })
    await vi.advanceTimersByTimeAsync(200)
    expect(onUpdateAvailable).toHaveBeenCalledWith({ version: '1.2.0' })
    stop2()
  })
})
