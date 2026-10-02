<template>
  <section
    v-if="availablePrices.length"
    class="space-y-3"
    data-testid="model-group-prices"
  >
    <div>
      <h4 class="text-sm font-semibold">
        各分组售价
      </h4>
      <p class="mt-1 text-xs text-muted-foreground">
        模型基础售价 × 分组统一倍率；实际调用和扣费使用 API Key 所选分组。
      </p>
    </div>
    <div class="overflow-x-auto rounded-lg border">
      <table class="w-full min-w-[620px] text-xs">
        <thead class="bg-muted/30">
          <tr class="border-b text-muted-foreground">
            <th class="p-3 text-left font-medium">
              分组
            </th><th class="p-3 text-right font-medium">
              倍率
            </th><th class="p-3 text-right font-medium">
              输入 $/M
            </th><th class="p-3 text-right font-medium">
              输出 $/M
            </th><th class="p-3 text-right font-medium">
              缓存创建 $/M
            </th><th class="p-3 text-right font-medium">
              缓存读取 $/M
            </th><th class="p-3 text-right font-medium">
              按次 $/次
            </th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="group in availablePrices"
            :key="group.id"
            class="border-b last:border-0"
            :data-pricing-group="group.id"
          >
            <td class="p-3">
              <span class="font-medium">{{ group.name }}</span><Badge
                v-if="!group.is_visible"
                variant="secondary"
                class="ml-1 text-[10px]"
              >
                隐藏
              </Badge>
            </td>
            <td class="p-3 text-right font-mono">
              {{ group.multiplier }}×
            </td>
            <td class="p-3 text-right font-mono">
              {{ formatModelPrice(group.default_tiered_pricing?.tiers?.[0]?.input_price_per_1m) }}
            </td>
            <td class="p-3 text-right font-mono">
              {{ formatModelPrice(group.default_tiered_pricing?.tiers?.[0]?.output_price_per_1m) }}
            </td>
            <td class="p-3 text-right font-mono">
              {{ formatModelPrice(group.default_tiered_pricing?.tiers?.[0]?.cache_creation_price_per_1m) }}
            </td>
            <td class="p-3 text-right font-mono">
              {{ formatModelPrice(group.default_tiered_pricing?.tiers?.[0]?.cache_read_price_per_1m) }}
            </td>
            <td class="p-3 text-right font-mono">
              {{ formatModelPrice(group.default_price_per_request) }}
            </td>
          </tr>
        </tbody>
      </table>
    </div>
    <p class="text-xs text-muted-foreground">
      上表显示首档价格；阶梯、缓存时长、图像和视频的完整分组价格可在下方展开。
    </p>
    <details
      v-for="group in availablePrices"
      :key="group.id"
      class="rounded-lg border p-3"
    >
      <summary class="cursor-pointer text-xs font-medium">
        {{ group.name }} · {{ group.multiplier }}× · 完整价格
      </summary>
      <div class="mt-3 space-y-3 text-xs">
        <div
          v-if="group.default_tiered_pricing?.tiers?.length"
          class="overflow-x-auto"
        >
          <table class="w-full min-w-[620px]">
            <thead class="text-muted-foreground">
              <tr>
                <th class="p-2 text-left font-medium">
                  上下文档位
                </th><th class="p-2 text-right font-medium">
                  输入 $/M
                </th><th class="p-2 text-right font-medium">
                  输出 $/M
                </th><th class="p-2 text-right font-medium">
                  缓存创建 $/M
                </th><th class="p-2 text-right font-medium">
                  缓存读取 $/M
                </th>
              </tr>
            </thead>
            <tbody>
              <template
                v-for="(tier, index) in group.default_tiered_pricing.tiers"
                :key="index"
              >
                <tr class="border-t">
                  <td class="p-2">
                    {{ index === 0 ? '0' : formatTokens(group.default_tiered_pricing.tiers[index - 1]?.up_to || 0) }}–{{ tier.up_to === null ? '不限' : formatTokens(tier.up_to) }}
                  </td><td class="p-2 text-right font-mono">
                    {{ formatModelPrice(tier.input_price_per_1m) }}
                  </td><td class="p-2 text-right font-mono">
                    {{ formatModelPrice(tier.output_price_per_1m) }}
                  </td><td class="p-2 text-right font-mono">
                    {{ formatModelPrice(tier.cache_creation_price_per_1m) }}
                  </td><td class="p-2 text-right font-mono">
                    {{ formatModelPrice(tier.cache_read_price_per_1m) }}
                  </td>
                </tr>
                <tr
                  v-for="ttl in tier.cache_ttl_pricing || []"
                  :key="ttl.ttl_minutes"
                >
                  <td
                    colspan="5"
                    class="px-2 pb-2 text-muted-foreground"
                  >
                    缓存创建 {{ ttl.ttl_minutes }} 分钟：<span class="font-mono">${{ formatModelPrice(ttl.cache_creation_price_per_1m) }}/M</span>
                  </td>
                </tr>
              </template>
            </tbody>
          </table>
        </div>
        <div v-if="group.default_price_per_request != null">
          按次：<span class="font-mono">${{ formatModelPrice(group.default_price_per_request) }}/次</span>
        </div>
        <div v-if="group.default_tiered_pricing?.image_output_price_default != null">
          图像默认：<span class="font-mono">${{ formatModelPrice(group.default_tiered_pricing.image_output_price_default) }}/张</span>
        </div>
        <div
          v-for="(qualities, size) in group.default_tiered_pricing?.image_output_prices || {}"
          :key="size"
          class="flex flex-wrap gap-3"
        >
          <span>图像 {{ size }}</span><span
            v-for="(price, quality) in qualities"
            :key="quality"
          >{{ quality }}：${{ formatPriceValue(price) }}/张</span>
        </div>
        <div
          v-for="(range, index) in group.default_tiered_pricing?.image_output_price_ranges || []"
          :key="index"
          class="flex flex-wrap gap-3"
        >
          <span>图像 {{ range.label || (range.up_to_pixels == null ? '不限像素' : `≤ ${range.up_to_pixels} 像素`) }}</span><span
            v-for="(price, quality) in range.prices"
            :key="quality"
          >{{ quality }}：${{ formatPriceValue(price) }}/张</span>
        </div>
        <div
          v-for="(price, resolution) in videoPrices(group)"
          :key="resolution"
        >
          视频 {{ resolution }}：<span class="font-mono">${{ formatPriceValue(price) }}/秒</span>
        </div>
        <ProcessingTierPricingSummary :pricing="group.default_tiered_pricing" />
      </div>
    </details>
  </section>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { Badge } from '@/components/ui'
import type { ModelGroupPrice } from '@/api/pricing-groups'
import { formatModelPrice, formatTokens } from '@/utils/format'
import ProcessingTierPricingSummary from './ProcessingTierPricingSummary.vue'

const props = defineProps<{ prices: ModelGroupPrice[] }>()
const availablePrices = computed(() => props.prices.filter(group => group.is_available))
function formatPriceValue(value: unknown) { return formatModelPrice(typeof value === 'number' ? value : null) }
function videoPrices(group: ModelGroupPrice): Record<string, unknown> {
  const billing = group.config?.billing as { video?: { price_per_second?: number; price_per_second_by_resolution?: Record<string, number> } } | undefined
  return { ...(billing?.video?.price_per_second != null ? { 默认: billing.video.price_per_second } : {}), ...billing?.video?.price_per_second_by_resolution }
}
</script>
