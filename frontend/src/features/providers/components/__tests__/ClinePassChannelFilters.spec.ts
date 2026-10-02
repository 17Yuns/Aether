import { afterEach, describe, expect, it, vi } from 'vitest'
import { createApp, defineComponent, h, nextTick, ref, type App } from 'vue'
import type { ClinePassConfig } from '@/api/endpoints'
import { createI18n } from '@/i18n'
import ClinePassChannelFilters from '../ClinePassChannelFilters.vue'

const { testModel, showError } = vi.hoisted(() => ({ testModel: vi.fn(), showError: vi.fn() }))
vi.mock('@/api/endpoints/providers', () => ({ testModel }))
vi.mock('@/composables/useToast', () => ({ useToast: () => ({ error: showError }) }))
vi.mock('@/components/ui', () => ({
  Button: defineComponent({ setup: (_, { attrs, slots }) => () => h('button', attrs, slots.default?.()) }),
  Label: defineComponent({ setup: (_, { slots }) => () => h('label', slots.default?.()) }),
  Input: defineComponent({
    props: { modelValue: String },
    emits: ['update:modelValue'],
    setup: (props, { attrs, emit }) => () => h('input', {
      ...attrs, value: props.modelValue,
      onInput: (event: Event) => emit('update:modelValue', (event.target as HTMLInputElement).value),
    }),
  }),
}))

const model = 'cline-pass/deepseek-v4.1-flash'
let app: App | undefined
let root: HTMLElement | undefined
afterEach(() => {
  app?.unmount()
  root?.remove()
  vi.clearAllMocks()
})

function mount(providerId?: string, availableChannels: string[] = []) {
  const config = ref<ClinePassConfig>({ models: { [model]: { only: ['baseten'], exclude: [], available_channels: availableChannels } } })
  root = document.createElement('div')
  document.body.appendChild(root)
  app = createApp(defineComponent({
    setup: () => () => h(ClinePassChannelFilters, {
      modelValue: config.value, providerId,
      'onUpdate:modelValue': value => { config.value = value },
    }),
  }))
  app.use(createI18n())
  app.mount(root)
  return { config, root }
}

function button(root: HTMLElement, text: string) {
  const found = [...root.querySelectorAll('button')].find(button => button.textContent?.trim() === text)
  if (!found) throw new Error(`Missing button: ${text}`)
  return found
}

async function selectOption(label: string) {
  const option = [...document.body.querySelectorAll('span')].find(span => span.textContent === label && span.parentElement?.querySelector('input[type="checkbox"]'))
  if (!option?.parentElement) throw new Error(`Missing option: ${label}`)
  option.parentElement.click()
  await nextTick()
}

async function closeDropdown() {
  document.body.querySelector<HTMLElement>('.fixed.inset-0')?.click()
  await nextTick()
}

describe('ClinePass channel filters', () => {
  it('probes without losing configured filters and marks ignored routing preferences', async () => {
    testModel.mockResolvedValue({ success: true, clinepass_probe: { available_channels: ['baseten', 'deepseek'], pipeline: 'planner', pinnable: false, pin_reason: '该模型忽略渠道商筛选参数' } })
    const { config, root } = mount('provider-1')
    button(root, '探测渠道商').click()
    await vi.waitFor(() => expect(config.value.models[model]?.pinnable).toBe(false))
    expect(testModel).toHaveBeenCalledWith({ provider_id: 'provider-1', model_name: model, clinepass_probe: true })
    expect(config.value.models[model]?.only).toEqual(['baseten'])
    expect(root.textContent).toContain('该模型忽略渠道商筛选参数')
    expect(button(root, '不排除渠道商').disabled).toBe(true)
  })

  it('selects known channels, orders priorities and prevents excluding every channel', async () => {
    const { config, root } = mount('provider-1', ['baseten', 'deepseek'])
    config.value.models[model]!.exclude = ['deepseek']
    await nextTick()
    button(root, 'baseten').click()
    await nextTick()
    await selectOption('deepseek')
    expect(config.value.models[model]?.only).toEqual(['baseten', 'deepseek'])
    expect(config.value.models[model]?.exclude).toEqual([])
    await closeDropdown()
    root.querySelector<HTMLButtonElement>('[aria-label="上移 deepseek"]')?.click()
    await nextTick()
    expect(config.value.models[model]?.only).toEqual(['deepseek', 'baseten'])
    button(root, '不排除渠道商').click()
    await nextTick()
    await selectOption('baseten')
    expect(config.value.models[model]?.only).toEqual(['deepseek'])
    expect(config.value.models[model]?.exclude).toEqual(['baseten'])
    await selectOption('deepseek')
    expect(showError).toHaveBeenCalledWith('至少保留一个可用渠道商')
    expect(config.value.models[model]?.exclude).toEqual(['baseten'])
  })

  it('disables probing until the provider is saved and rejects non-subscription model IDs', async () => {
    const { config, root } = mount()
    expect(button(root, '探测渠道商').disabled).toBe(true)
    const input = root.querySelector<HTMLInputElement>('[aria-label="ClinePass 模型名称"]')!
    input.value = 'deepseek/deepseek-v4.1-flash'
    input.dispatchEvent(new Event('input'))
    await nextTick()
    button(root, '添加模型').click()
    expect(showError).toHaveBeenCalledWith('ClinePass 套餐模型必须使用 cline-pass/ 开头的模型 ID')
    expect(Object.keys(config.value.models)).toEqual([model])
  })
})
