import { afterEach, describe, expect, it, vi } from 'vitest'
import { createApp, defineComponent, h, nextTick, ref, type App } from 'vue'
import type { ClinePassConfig } from '@/api/endpoints'
import ClinePassChannelFilters from '../ClinePassChannelFilters.vue'

const { testModel, showError } = vi.hoisted(() => ({ testModel: vi.fn(), showError: vi.fn() }))
vi.mock('@/api/endpoints', () => ({ testModel }))
vi.mock('@/composables/useToast', () => ({ useToast: () => ({ error: showError }) }))
vi.mock('@/components/ui', () => ({
  Button: defineComponent({ setup: (_, { attrs, slots }) => () => h('button', attrs, slots.default?.()) }),
  Label: defineComponent({ setup: (_, { slots }) => () => h('label', slots.default?.()) }),
  Input: defineComponent({ props: { modelValue: String }, emits: ['update:modelValue'], setup: (props, { attrs, emit }) => () => h('input', { ...attrs, value: props.modelValue, onInput: (event: Event) => emit('update:modelValue', (event.target as HTMLInputElement).value) }) }),
}))

let app: App | undefined
let root: HTMLElement | undefined
afterEach(() => { app?.unmount(); root?.remove(); vi.clearAllMocks() })

function mount(providerId?: string) {
  const config = ref<ClinePassConfig>({ models: { test: { only: ['baseten'], exclude: [], available_channels: [] } } })
  root = document.createElement('div')
  document.body.appendChild(root)
  app = createApp(defineComponent({ setup: () => () => h(ClinePassChannelFilters, { modelValue: config.value, providerId, 'onUpdate:modelValue': value => { config.value = value } }) }))
  app.mount(root)
  return { config, root }
}

describe('ClinePass channel filters', () => {
  it('probes without losing configured filters and marks ignored routing preferences', async () => {
    testModel.mockResolvedValue({ success: true, clinepass_probe: { available_channels: ['baseten', 'deepseek'], pipeline: 'planner', pinnable: false, pin_reason: '该模型忽略渠道商筛选参数' } })
    const { config, root } = mount('provider-1')
    const button = [...root.querySelectorAll('button')].find(button => button.textContent?.includes('探测渠道商'))
    button?.click()
    await vi.waitFor(() => expect(config.value.models.test?.pinnable).toBe(false))
    expect(testModel).toHaveBeenCalledWith({ provider_id: 'provider-1', model_name: 'test', clinepass_probe: true })
    expect(config.value.models.test?.only).toEqual(['baseten'])
    expect(root.textContent).toContain('该模型忽略渠道商筛选参数')
  })

  it('parses channel lists and disables probing before a provider is saved', async () => {
    const { config, root } = mount()
    const inputs = root.querySelectorAll('input')
    const only = inputs[1]
    if (!only) throw new Error('Missing allow-list input')
    only.value = 'baseten, '
    only.dispatchEvent(new Event('input'))
    await nextTick()
    expect(only.value).toBe('baseten, ')
    only.value = 'baseten， deepseek, baseten'
    only.dispatchEvent(new Event('input'))
    await nextTick()
    expect(config.value.models.test?.only).toEqual(['baseten', 'deepseek'])
    const button = [...root.querySelectorAll('button')].find(button => button.textContent?.includes('探测渠道商'))
    expect(button?.disabled).toBe(true)
  })
})
