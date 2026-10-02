import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createApp, defineComponent, h, nextTick, ref, type App } from 'vue'
import StandaloneKeyFormDialog, { type StandaloneKeyFormData } from '../StandaloneKeyFormDialog.vue'

const mocks = vi.hoisted(() => ({ groups: vi.fn(), providers: vi.fn(), models: vi.fn(), formats: vi.fn() }))
vi.mock('@/api/pricing-groups', () => ({ pricingGroupsApi: { getAdmin: mocks.groups } }))
vi.mock('@/api/endpoints/providers', () => ({ getProvidersSummary: mocks.providers }))
vi.mock('@/api/global-models', () => ({ getGlobalModels: mocks.models }))
vi.mock('@/api/admin', () => ({ adminApi: { getApiFormats: mocks.formats } }))

const mounted: Array<{ app: App; root: HTMLElement }> = []
async function flush() {
  for (let i = 0; i < 5; i++) { await Promise.resolve(); await nextTick() }
}
async function mountDialog(key?: StandaloneKeyFormData) {
  const root = document.createElement('div')
  document.body.appendChild(root)
  const open = ref(false)
  const submit = vi.fn()
  const app = createApp(defineComponent({
    setup: () => () => h(StandaloneKeyFormDialog, { open: open.value, apiKey: key ?? null, onSubmit: submit }),
  }))
  app.mount(root)
  mounted.push({ app, root })
  open.value = true
  await flush()
  return { submit }
}
function submitButton() {
  return [...document.body.querySelectorAll<HTMLButtonElement>('button')]
    .find(button => ['创建', '更新'].includes(button.textContent?.trim() || ''))!
}
beforeEach(() => {
  vi.clearAllMocks()
  mocks.providers.mockResolvedValue({ items: [] })
  mocks.models.mockResolvedValue({ models: [] })
  mocks.formats.mockResolvedValue({ formats: [] })
  mocks.groups.mockResolvedValue({ enabled: true, default_group_id: 'default', groups: [
    { id: 'default', name: '默认', multiplier: 1, is_visible: true },
    { id: 'private', name: '专属', multiplier: 0.1, is_visible: false },
  ] })
})
afterEach(() => {
  for (const { app, root } of mounted.splice(0)) { app.unmount(); root.remove() }
})
describe('standalone key pricing groups', () => {
  it('allows administrators to select and submit a hidden group', async () => {
    const { submit } = await mountDialog()
    const select = document.querySelector<HTMLSelectElement>('#standalone-key-pricing-group')!
    expect(select.value).toBe('default')
    expect(select.textContent).toContain('隐藏')
    select.value = 'private'
    select.dispatchEvent(new Event('change', { bubbles: true }))
    await nextTick()
    submitButton().click()
    expect(submit).toHaveBeenCalledWith(expect.objectContaining({ pricing_group_id: 'private' }))
  })
  it('preserves an existing group and can reset it to the default', async () => {
    const { submit } = await mountDialog({ id: 'key-1', name: 'Existing', auto_delete_on_expiry: false, feature_settings: { pricing_group_id: 'private' } })
    const select = document.querySelector<HTMLSelectElement>('#standalone-key-pricing-group')!
    expect(select.value).toBe('private')
    submitButton().click()
    expect(submit).toHaveBeenLastCalledWith(expect.objectContaining({ pricing_group_id: 'private' }))
    select.value = ''
    select.dispatchEvent(new Event('change', { bubbles: true }))
    await nextTick()
    submitButton().click()
    expect(submit).toHaveBeenLastCalledWith(expect.objectContaining({ pricing_group_id: null }))
  })
  it('blocks submission when groups cannot load and allows retry', async () => {
    mocks.groups.mockRejectedValueOnce(new Error('Group service unavailable'))
    const { submit } = await mountDialog()
    expect(submitButton().disabled).toBe(true)
    submitButton().click()
    expect(submit).not.toHaveBeenCalled()
    const retry = [...document.body.querySelectorAll<HTMLButtonElement>('button')]
      .find(button => button.textContent?.trim() === '重试')!
    retry.click()
    await flush()
    expect(submitButton().disabled).toBe(false)
    submitButton().click()
    expect(submit).toHaveBeenCalledWith(expect.objectContaining({ pricing_group_id: 'default' }))
  })
})
