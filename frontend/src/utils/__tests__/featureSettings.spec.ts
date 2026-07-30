import { describe, expect, it } from 'vitest'
import {
  mergeApiKeyBillingSourceMode,
  readApiKeyBillingMultiplierMode,
  readApiKeyBillingSourceMode,
} from '../featureSettings'

describe('API key billing feature settings', () => {
  it('treats missing and invalid billing settings as legacy defaults', () => {
    expect(readApiKeyBillingSourceMode(null)).toBe('auto')
    expect(readApiKeyBillingSourceMode({ billing_source: { mode: 'invalid' } })).toBe('auto')
    expect(readApiKeyBillingMultiplierMode(null)).toBe('custom')
    expect(readApiKeyBillingMultiplierMode({ billing_multiplier: { mode: 'inherit' } })).toBe('inherit')
  })

  it('preserves unrelated settings while changing the billing source', () => {
    const current = {
      billing_multiplier: { mode: 'inherit' },
      chat_pii_redaction: { enabled: true, inject_model_instruction: false },
    }

    const packageOnly = mergeApiKeyBillingSourceMode(current, 'package')
    expect(packageOnly).toEqual({
      ...current,
      billing_source: { mode: 'package' },
    })

    expect(mergeApiKeyBillingSourceMode(packageOnly, 'auto')).toEqual(current)
  })

  it('omits the canonical auto source when no other settings exist', () => {
    expect(mergeApiKeyBillingSourceMode({ billing_source: { mode: 'wallet' } }, 'auto')).toBeNull()
  })
})
