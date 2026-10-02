<template>
  <div
    class="mt-2 space-y-2 rounded-md bg-muted/30 p-2"
    data-testid="clinepass-quota"
  >
    <div class="flex items-center justify-between text-[11px]">
      <span>{{ quota?.plan_type || metadata?.plan_type || 'ClinePass 套餐' }}</span>
      <span class="text-muted-foreground">剩余额度</span>
    </div>
    <template
      v-for="window in windows"
      :key="window.code"
    >
      <ProviderQuotaProgressRow
        v-if="window.remainingPercent != null"
        :label="window.label"
        :remaining-percent="window.remainingPercent"
        :bar-class="window.remainingPercent > 20 ? 'bg-emerald-500' : window.remainingPercent > 0 ? 'bg-amber-500' : 'bg-destructive'"
        :reset-text="window.resetAt ? `${formatReset(window.resetAt)} 重置` : null"
      />
      <div
        v-else
        class="flex justify-between text-[10px] text-muted-foreground"
      >
        <span>{{ window.label }}</span><span>额度未知</span>
      </div>
    </template>
    <div
      v-if="metadata?.current_period_end"
      class="text-[10px] text-muted-foreground"
    >
      套餐到期: {{ formatExpiry(metadata.current_period_end) }}
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { ClinePassUpstreamMetadata, QuotaStatusSnapshot } from '@/api/endpoints/types'
import { getClinePassQuotaWindows } from '../utils/clinepassQuota'
import ProviderQuotaProgressRow from './ProviderQuotaProgressRow.vue'

const props = defineProps<{
  quota?: QuotaStatusSnapshot | null
  metadata?: ClinePassUpstreamMetadata | null
}>()
const windows = computed(() => getClinePassQuotaWindows(props.quota))
const formatReset = (seconds: number) => new Date(seconds * 1000).toLocaleString()
function formatExpiry(value: string) {
  const date = new Date(value)
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString()
}
</script>
