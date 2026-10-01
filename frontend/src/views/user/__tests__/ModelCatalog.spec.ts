import { afterEach, describe, expect, it, vi } from 'vitest'
import { createApp, nextTick, type App } from 'vue'

import type { PublicGlobalModel } from '@/api/public-models'
import UserModelDetailDrawer from '../components/UserModelDetailDrawer.vue'
import ModelCatalog from '../ModelCatalog.vue'

const pricingApiMock = vi.hoisted(() => ({ getVisible: vi.fn(), getAvailableModels: vi.fn() }))
vi.mock('@/api/pricing-groups', () => ({ pricingGroupsApi: { getVisible: pricingApiMock.getVisible } }))
vi.mock('@/api/me', () => ({ meApi: { getAvailableModels: pricingApiMock.getAvailableModels } }))
vi.mock('@/composables/useToast', () => ({ useToast: () => ({ error: vi.fn() }) }))

vi.mock('@/composables/useClipboard', () => ({
  useClipboard: () => ({
    copyToClipboard: vi.fn(),
  }),
}))

const mountedApps: Array<{ app: App, root: HTMLElement }> = []

function model(overrides: Partial<PublicGlobalModel> = {}): PublicGlobalModel {
  return {
    id: 'gm-test',
    name: 'gpt-5',
    display_name: 'GPT 5',
    is_active: true,
    default_tiered_pricing: null,
    default_price_per_request: null,
    supported_capabilities: ['chat'],
    config: null,
    usage_count: 0,
    ...overrides,
  }
}

function mountDrawer(selectedModel: PublicGlobalModel) {
  const root = document.createElement('div')
  document.body.appendChild(root)
  const app = createApp(UserModelDetailDrawer, {
    open: true,
    model: selectedModel,
    'onUpdate:open': vi.fn(),
  })
  app.mount(root)
  mountedApps.push({ app, root })
  return root
}

afterEach(() => {
  for (const { app, root } of mountedApps.splice(0)) {
    app.unmount()
    root.remove()
  }
  document.body.innerHTML = ''
})

describe('user model catalog detail drawer', () => {
  it('requests prices for the selected group and renders the server prices without multiplying twice', async () => {
    pricingApiMock.getVisible.mockResolvedValue({
      enabled: true,
      default_group_id: 'default',
      groups: [
        { id: 'default', name: '默认', multiplier: 1, is_visible: true },
        { id: 'vip', name: 'VIP', multiplier: 0.5, is_visible: true },
      ],
    })
    pricingApiMock.getAvailableModels.mockResolvedValue({ models: [model({
      default_tiered_pricing: { tiers: [{ up_to: null, input_price_per_1m: 1.5, output_price_per_1m: 0 }] },
    })], total: 1 })
    const root = document.createElement('div')
    document.body.appendChild(root)
    const app = createApp(ModelCatalog)
    app.mount(root)
    mountedApps.push({ app, root })
    await nextTick()
    await new Promise(resolve => setTimeout(resolve, 0))
    await nextTick()
    expect(pricingApiMock.getAvailableModels).toHaveBeenLastCalledWith({ limit: 1000, pricing_group_id: 'default' })
    const selector = root.querySelector<HTMLSelectElement>('select[aria-label="定价分组"]')!
    selector.value = 'vip'
    selector.dispatchEvent(new Event('change', { bubbles: true }))
    await nextTick()
    await new Promise(resolve => setTimeout(resolve, 0))
    await nextTick()
    expect(pricingApiMock.getAvailableModels).toHaveBeenLastCalledWith({ limit: 1000, pricing_group_id: 'vip' })
    expect(root.textContent).toContain('1.50')
    expect(root.textContent).toContain('0.00')
    expect(root.textContent).not.toContain('0.75')
  })

  it('does not render model mapping fields for ordinary users', async () => {
    mountDrawer(model({
      config: {
        description: 'User visible description',
        model_mappings: ['gpt-5-upstream'],
        provider_model_mappings: [{ name: 'provider-gpt-5' }],
      },
    }))
    await nextTick()

    const text = document.body.textContent || ''
    expect(text).toContain('GPT 5')
    expect(text).toContain('User visible description')
    expect(text).not.toContain('模型映射')
    expect(text).not.toContain('gpt-5-upstream')
    expect(text).not.toContain('provider-gpt-5')
  })
})
