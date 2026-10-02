import { afterEach, describe, expect, it } from 'vitest'
import { createApp, type App } from 'vue'
import ModelGroupPrices from '../ModelGroupPrices.vue'

let app: App | undefined
afterEach(() => { app?.unmount(); document.body.innerHTML = '' })
describe('model group prices', () => {
  it('shows the server prices once, including zero, small cache prices and unavailable groups', () => {
    const root = document.createElement('div')
    document.body.appendChild(root)
    app = createApp(ModelGroupPrices, { prices: [
      { id: 'deepseek', name: 'DeepSeek', multiplier: 0.1, is_visible: true, is_available: true,
        default_tiered_pricing: { tiers: [{ up_to: null, input_price_per_1m: 0.3, output_price_per_1m: 0, cache_read_price_per_1m: 0.000001,
          cache_ttl_pricing: [{ ttl_minutes: 60, cache_creation_price_per_1m: 0.6 }] }] }, default_price_per_request: 0.002 },
      { id: 'claude', name: 'Claude', multiplier: 1.1, is_visible: true, is_available: false },
      { id: 'private', name: '专属', multiplier: 0.5, is_visible: false, is_available: true, default_price_per_request: 0.01 },
    ] })
    app.mount(root)
    const deepseek = root.querySelector('[data-pricing-group="deepseek"]')!
    expect(deepseek.textContent).toContain('0.30')
    expect(deepseek.textContent).toContain('0.00')
    expect(deepseek.textContent).toContain('0.000001')
    expect(deepseek.textContent).not.toContain('0.03')
    expect(root.querySelector('[data-pricing-group="claude"]')?.textContent).toContain('未开放')
    expect(root.querySelector('[data-pricing-group="private"]')?.textContent).toContain('隐藏')
    expect(root.textContent).toContain('60 分钟')
  })
})
