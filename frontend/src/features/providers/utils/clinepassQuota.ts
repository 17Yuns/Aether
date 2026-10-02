import type { QuotaStatusSnapshot } from '@/api/endpoints/types'

export function getClinePassQuotaWindows(quota?: QuotaStatusSnapshot | null) {
  return ([['five_hour', '5h'], ['weekly', '7d'], ['monthly', '30d']] as const).map(([code, label]) => {
    const window = quota?.windows?.find(window => window.code === code)
    const remaining = typeof window?.remaining_ratio === 'number'
      ? window.remaining_ratio
      : typeof window?.used_ratio === 'number' ? 1 - window.used_ratio : null
    return {
      code, label,
      remainingPercent: remaining != null && Number.isFinite(remaining)
        ? Math.min(100, Math.max(0, remaining * 100)) : null,
      resetAt: typeof window?.reset_at === 'number' ? window.reset_at : null,
    }
  })
}
