import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createApp, defineComponent, h, nextTick, type App, type PropType } from 'vue'
import type { ClinePassModelFilter, Model } from '@/api/endpoints'
import ProviderModelFormDialog from '../ProviderModelFormDialog.vue'

const mocks = vi.hoisted(() => ({
  createModel: vi.fn(), updateModel: vi.fn(), getProviderModels: vi.fn(),
  getProvider: vi.fn(), updateProvider: vi.fn(),
  createGlobalModel: vi.fn(), getGlobalModel: vi.fn(), listGlobalModels: vi.fn(),
  fetchModels: vi.fn(), error: vi.fn(),
}))
vi.mock('@/api/endpoints/models', () => mocks)
vi.mock('@/api/endpoints/providers', () => mocks)
vi.mock('@/api/global-models', () => mocks)
vi.mock('@/features/providers/composables/useUpstreamModelsCache', () => ({ useUpstreamModelsCache: () => ({ fetchModels: mocks.fetchModels }) }))
vi.mock('@/composables/useToast', () => ({ useToast: () => ({ error: mocks.error, success: vi.fn() }) }))
vi.mock('../ClinePassModelChannelFilter.vue', () => ({ default: defineComponent({
  props: { modelValue: Object as PropType<ClinePassModelFilter> },
  emits: ['update:modelValue'],
  setup: (props, { emit }) => () => h('button', {
    onClick: () => emit('update:modelValue', { ...props.modelValue, only: ['baseten'], exclude: [], available_channels: ['baseten', 'deepseek'], pipeline: 'planner' }),
  }, '选择 baseten'),
}) }))

const modelName = 'cline-pass/deepseek-v4.1-flash'
const rule = { only: ['deepseek'], exclude: [], available_channels: ['baseten', 'deepseek'], pipeline: 'planner' }
const editedModel = {
  id: 'model-1', provider_id: 'provider-1', provider_model_name: modelName,
  global_model_id: 'global-1', is_active: true,
} as Model
let app: App | undefined
let root: HTMLElement | undefined
function mount(editingModel: Model | null) {
  root = document.createElement('div')
  document.body.appendChild(root)
  app = createApp(defineComponent({ setup: () => () => h(ProviderModelFormDialog, {
    open: true, providerId: 'provider-1', providerType: 'clinepass', editingModel,
  }) }))
  app.mount(root)
}
function button(text: string) {
  const found = [...document.body.querySelectorAll('button')].find(button => button.textContent?.trim() === text)
  if (!found) throw new Error(`Missing button: ${text}`)
  return found
}
async function settle() {
  for (let index = 0; index < 8; index++) { await Promise.resolve(); await nextTick() }
}
async function input(selector: string, value: string) {
  const element = document.body.querySelector<HTMLInputElement>(selector)
  if (!element) throw new Error(`Missing input: ${selector}`)
  element.value = value
  element.dispatchEvent(new Event('input', { bubbles: true }))
  await nextTick()
}
beforeEach(() => {
  vi.resetAllMocks()
  mocks.getProvider.mockResolvedValue({ clinepass: { models: { [modelName]: rule } } })
  mocks.fetchModels.mockResolvedValue({ models: [{ id: modelName }, { id: 'deepseek/deepseek-v4.1-flash' }] })
  mocks.getProviderModels.mockResolvedValue([])
  mocks.listGlobalModels.mockResolvedValue({ models: [] })
  mocks.createGlobalModel.mockResolvedValue({ id: 'global-1', name: 'deepseek-v4.1-flash' })
  mocks.createModel.mockResolvedValue({})
  mocks.updateModel.mockResolvedValue({})
  mocks.updateProvider.mockResolvedValue({})
})
afterEach(() => { app?.unmount(); root?.remove(); document.body.innerHTML = '' })

describe('ClinePass model configuration', () => {
  it('edits model settings without duplicating mapping and channel controls', async () => {
    mount(editedModel)
    await settle()
    expect(document.body.textContent).not.toContain('渠道商筛选')
    expect(document.body.textContent).not.toContain('选择 baseten')
    expect(mocks.getProvider).not.toHaveBeenCalled()
    expect(mocks.fetchModels).not.toHaveBeenCalled()
    button('保存').click()
    await settle()
    expect(mocks.updateModel).toHaveBeenCalledWith('provider-1', 'model-1', expect.any(Object))
    expect(mocks.updateProvider).not.toHaveBeenCalled()
  })

  it('adds a local model independently of its upstream mapping', async () => {
    mount(null)
    await settle()
    button('手动添加').click()
    await nextTick()
    await input('#manual-global-model-name', 'deepseek-v4.1-flash')
    button('添加').click()
    await settle()
    expect(mocks.createModel).toHaveBeenCalledWith('provider-1', expect.objectContaining({
      global_model_id: 'global-1', provider_model_name: 'deepseek-v4.1-flash',
    }))
    expect(mocks.updateProvider).not.toHaveBeenCalled()
  })
})
