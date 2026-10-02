import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createApp, nextTick, type App } from 'vue'
import PricingGroupsManagement from '../PricingGroupsManagement.vue'

const mocks = vi.hoisted(() => ({ getAdmin: vi.fn(), save: vi.fn(), models: vi.fn(), providers: vi.fn(), modelProviders: vi.fn(), error: vi.fn() }))
vi.mock('@/api/pricing-groups', () => ({ pricingGroupsApi: { getAdmin: mocks.getAdmin, save: mocks.save } }))
vi.mock('@/api/global-models', () => ({ getGlobalModels: mocks.models, getGlobalModelProviders: mocks.modelProviders }))
vi.mock('@/api/endpoints/providers', () => ({ getProvidersSummary: mocks.providers }))
vi.mock('@/composables/useToast', () => ({ useToast: () => ({ success: vi.fn(), error: mocks.error }) }))
const mounted: Array<{ app: App; root: HTMLElement }> = []
async function flush() { await new Promise(resolve => setTimeout(resolve, 0)); await nextTick() }
async function mountPage() {
  const root = document.createElement('div')
  document.body.appendChild(root)
  const app = createApp(PricingGroupsManagement)
  app.mount(root)
  mounted.push({ app, root })
  await flush()
  return root
}
function checkbox(root: HTMLElement, label: string) { return root.querySelector<HTMLInputElement>(`input[aria-label="${label}"]`)! }
async function select(root: HTMLElement, label: string, checked: boolean) {
  const input = checkbox(root, label)
  input.checked = checked
  input.dispatchEvent(new Event('change', { bubbles: true }))
  await nextTick()
}
beforeEach(() => {
  vi.clearAllMocks()
  mocks.getAdmin.mockResolvedValue({ enabled: true, default_group_id: 'deepseek', groups: [
    { id: 'deepseek', name: 'DeepSeek', multiplier: 0.1, is_visible: true, model_access: [{ global_model_id: 'gm-deepseek', provider_ids: ['a'] }] },
    { id: 'claude', name: 'Claude', multiplier: 1.1, is_visible: true, model_access: [] },
  ] })
  mocks.models.mockResolvedValue({ total: 2, models: [
    { id: 'gm-deepseek', name: 'deepseek-flash', display_name: 'DeepSeek' },
    { id: 'gm-claude', name: 'claude-sonnet', display_name: 'Claude' },
  ] })
  mocks.providers.mockResolvedValue({ total: 3, items: [{ id: 'a', name: '渠道 A' }, { id: 'b', name: '渠道 B' }, { id: 'c', name: '渠道 C' }] })
  mocks.modelProviders.mockResolvedValue({ total: 3, providers: [
    { provider_id: 'a', provider_name: '渠道 A', target_model: 'deepseek-a', is_active: true },
    { provider_id: 'b', provider_name: '渠道 B', target_model: 'deepseek-b', is_active: true },
    { provider_id: 'c', provider_name: '渠道 C', target_model: 'deepseek-c', is_active: true },
  ] })
  mocks.save.mockImplementation(async config => JSON.parse(JSON.stringify(config)))
})
afterEach(() => { for (const { app, root } of mounted.splice(0)) { app.unmount(); root.remove() } })

describe('pricing group management', () => {
  it('expands a model without selecting all providers and keeps overlapping group selections independent', async () => {
    const root = await mountPage()
    root.querySelector<HTMLButtonElement>('[aria-label="展开模型 deepseek-flash"]')!.click()
    await flush()
    expect(checkbox(root, 'deepseek-flash · 渠道 A').checked).toBe(true)
    expect(checkbox(root, 'deepseek-flash · 渠道 B').checked).toBe(false)
    expect(checkbox(root, 'deepseek-flash · 渠道 C').checked).toBe(false)
    await select(root, 'deepseek-flash · 渠道 B', true)
    root.querySelector<HTMLButtonElement>('[aria-label="编辑分组 Claude"]')!.click()
    await nextTick()
    expect(checkbox(root, 'deepseek-flash · 渠道 A').checked).toBe(false)
    expect(checkbox(root, 'deepseek-flash · 渠道 B').checked).toBe(false)
    await select(root, 'deepseek-flash · 渠道 A', true)
    const save = [...root.querySelectorAll('button')].find(button => button.textContent?.includes('保存配置'))!
    save.click()
    await flush()
    const config = mocks.save.mock.calls[0]![0]
    expect(config.groups[0]).toMatchObject({ multiplier: 0.1, model_access: [{ global_model_id: 'gm-deepseek', provider_ids: ['a', 'b'] }] })
    expect(config.groups[1]).toMatchObject({ multiplier: 1.1, model_access: [{ global_model_id: 'gm-deepseek', provider_ids: ['a'] }] })
    expect(config.groups[0].model_access[0]).not.toHaveProperty('multiplier')
  })
  it('shows saved model/provider names even when loading selectable providers fails', async () => {
    mocks.modelProviders.mockRejectedValue(new Error('unavailable'))
    const root = await mountPage()
    expect(root.textContent).toContain('deepseek-flash')
    expect(root.textContent).toContain('渠道 A')
    root.querySelector<HTMLButtonElement>('[aria-label="展开模型 deepseek-flash"]')!.click()
    await flush()
    expect(root.textContent).toContain('重试')
    expect(root.textContent).toContain('渠道 A')
    expect(mocks.save).not.toHaveBeenCalled()
  })
})
