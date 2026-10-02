import { afterEach, describe, expect, it, vi } from 'vitest'
import { createApp, defineComponent, h, nextTick, type App } from 'vue'
import ProviderKeyBatchImportDialog from '../ProviderKeyBatchImportDialog.vue'

const mocks = vi.hoisted(() => ({ batchImportPoolKeys: vi.fn() }))
vi.mock('@/api/endpoints/pool', () => mocks)
vi.mock('@/composables/useToast', () => ({ useToast: () => ({ error: vi.fn(), success: vi.fn(), warning: vi.fn() }) }))
vi.mock('../ProviderKeyImportSettingsFields.vue', () => ({ default: defineComponent({ setup: () => () => h('div') }) }))
let app: App | undefined
let root: HTMLElement | undefined
afterEach(() => { app?.unmount(); root?.remove(); document.body.innerHTML = ''; vi.clearAllMocks() })
function button(text: string) {
  const found = [...document.body.querySelectorAll('button')].find(button => button.textContent?.trim() === text)
  if (!found) throw new Error(`Missing button: ${text}`)
  return found
}
describe('API key pool import', () => {
  it('imports pasted keys with generated names and chat format without OAuth', async () => {
    mocks.batchImportPoolKeys.mockResolvedValue({ imported: 2, errors: [] })
    root = document.createElement('div')
    document.body.appendChild(root)
    let saved = false
    app = createApp(defineComponent({ setup: () => () => h(ProviderKeyBatchImportDialog, {
      open: true, providerId: 'clinepass-1', providerName: 'ClinePass',
      allowBareKeys: true, availableApiFormats: ['openai:chat'], onSaved: () => { saved = true },
    }) }))
    app.mount(root)
    await nextTick()
    const textarea = document.body.querySelector<HTMLTextAreaElement>('#provider-key-batch-input')!
    textarea.value = 'sk-first\nsk-second'
    textarea.dispatchEvent(new Event('input', { bubbles: true }))
    await nextTick()
    button('下一步：统一配置').click()
    await nextTick()
    button('下一步：逐项确认').click()
    await nextTick()
    button('导入 2 个 Key').click()
    await vi.waitFor(() => expect(saved).toBe(true))
    expect(mocks.batchImportPoolKeys).toHaveBeenCalledWith('clinepass-1', expect.objectContaining({
      keys: [
        { name: expect.stringMatching(/^ClinePass-\d+-1$/), api_key: 'sk-first', auth_type: 'api_key' },
        { name: expect.stringMatching(/^ClinePass-\d+-2$/), api_key: 'sk-second', auth_type: 'api_key' },
      ],
      api_formats: ['openai:chat'],
    }))
  })
})
