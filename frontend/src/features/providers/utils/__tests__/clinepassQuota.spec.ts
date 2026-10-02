import { describe, expect, it } from 'vitest'
import { getClinePassQuotaWindows } from '../clinepassQuota'
import type { QuotaStatusSnapshot } from '@/api/endpoints/types'

describe('ClinePass quota windows', () => {
  it('keeps all three windows independent and uses remaining percentages', () => {
    const quota = { windows: [
      { code: 'monthly', used_ratio: 0.4, reset_at: 300 },
      { code: 'five_hour', remaining_ratio: 0.8, reset_at: 100 },
      { code: 'weekly', used_ratio: 1, reset_at: 200 },
    ] } as QuotaStatusSnapshot
    expect(getClinePassQuotaWindows(quota)).toEqual([
      { code: 'five_hour', label: '5h', remainingPercent: 80, resetAt: 100 },
      { code: 'weekly', label: '7d', remainingPercent: 0, resetAt: 200 },
      { code: 'monthly', label: '30d', remainingPercent: 60, resetAt: 300 },
    ])
  })

  it('preserves unknown quota instead of displaying zero or full remaining', () => {
    const windows = getClinePassQuotaWindows({ windows: [{ code: 'weekly', remaining_ratio: null, used_ratio: null }] } as QuotaStatusSnapshot)
    expect(windows).toHaveLength(3)
    expect(windows.every(window => window.remainingPercent === null)).toBe(true)
  })
})
