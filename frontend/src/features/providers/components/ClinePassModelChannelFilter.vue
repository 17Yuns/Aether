<template>
  <section
    class="space-y-3 rounded-lg border p-3"
    data-testid="clinepass-model-channel-filter"
  >
    <div class="flex items-center justify-between gap-2">
      <Label v-if="showHeading">渠道商筛选</Label>
      <Button
        type="button"
        size="sm"
        variant="outline"
        :disabled="!providerId || !modelName.trim() || probing"
        @click="probe"
      >
        {{ probing ? '探测中…' : '探测渠道商' }}
      </Button>
    </div>
    <p class="text-xs text-muted-foreground">
      探测此模型的可用渠道商后直接选择。探测最多发送两次模型请求，会消耗套餐额度。
    </p>
    <p
      v-if="modelValue.pinnable === false"
      class="text-xs text-amber-600"
    >
      {{ modelValue.pin_reason || '该模型不支持渠道商筛选' }}
    </p>
    <div class="space-y-1.5">
      <Label class="text-xs">允许渠道商</Label>
      <MultiSelect
        :model-value="modelValue.only"
        :options="options"
        :disabled="modelValue.pinnable === false || !options.length"
        placeholder="由 ClinePass 自动选择"
        empty-text="请先探测此模型的渠道商"
        search-placeholder="搜索渠道商"
        teleport
        @update:model-value="selectAllowed"
      />
      <div
        v-if="modelValue.only.length"
        class="space-y-1"
      >
        <div
          v-for="(channel, index) in modelValue.only"
          :key="channel"
          class="flex items-center gap-2 text-xs"
        >
          <span class="text-muted-foreground">{{ index + 1 }}.</span>
          <span class="min-w-0 flex-1 truncate">{{ channel }}</span>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            :aria-label="`上移 ${channel}`"
            :disabled="index === 0 || modelValue.pinnable === false"
            @click="moveChannel(index, -1)"
          >
            <ArrowUp class="h-3 w-3" />
          </Button>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            :aria-label="`下移 ${channel}`"
            :disabled="index === modelValue.only.length - 1 || modelValue.pinnable === false"
            @click="moveChannel(index, 1)"
          >
            <ArrowDown class="h-3 w-3" />
          </Button>
        </div>
        <p class="text-xs text-muted-foreground">
          按以上顺序优先请求；留空时由 ClinePass 自动选择。
        </p>
      </div>
    </div>
    <div class="space-y-1.5">
      <Label class="text-xs">排除渠道商</Label>
      <MultiSelect
        :model-value="modelValue.exclude"
        :options="options"
        :disabled="modelValue.pinnable === false || !options.length"
        placeholder="不排除渠道商"
        search-placeholder="搜索渠道商"
        teleport
        @update:model-value="selectExcluded"
      />
    </div>
    <p
      v-if="!options.length"
      class="text-xs text-muted-foreground"
    >
      尚未获取此模型的渠道商，点击“探测渠道商”获取。
    </p>
  </section>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import { ArrowDown, ArrowUp } from 'lucide-vue-next'
import { Button, Label } from '@/components/ui'
import MultiSelect from '@/components/common/MultiSelect.vue'
import { testModel } from '@/api/endpoints/providers'
import type { ClinePassModelFilter } from '@/api/endpoints'
import { useToast } from '@/composables/useToast'
import { parseApiError } from '@/utils/errorParser'

const props = withDefaults(defineProps<{
  modelValue: ClinePassModelFilter
  providerId?: string
  modelName: string
  showHeading?: boolean
}>(), { showHeading: true, providerId: undefined })
const emit = defineEmits<{ 'update:modelValue': [value: ClinePassModelFilter] }>()
const probing = ref(false)
const { error: showError } = useToast()
const options = computed(() => [...new Set([
  ...props.modelValue.available_channels,
  ...props.modelValue.only,
  ...props.modelValue.exclude,
])].map(value => ({ value, label: value })))

function selectAllowed(only: string[]) {
  emit('update:modelValue', { ...props.modelValue, only, exclude: props.modelValue.exclude.filter(channel => !only.includes(channel)) })
}
function selectExcluded(exclude: string[]) {
  const only = props.modelValue.only.filter(channel => !exclude.includes(channel))
  if (!only.length && options.value.length && options.value.every(option => exclude.includes(option.value))) {
    showError('至少保留一个可用渠道商')
    return
  }
  emit('update:modelValue', { ...props.modelValue, only, exclude })
}
function moveChannel(index: number, offset: number) {
  const only = [...props.modelValue.only]
  const destination = index + offset
  if (destination < 0 || destination >= only.length) return
  const [channel] = only.splice(index, 1)
  if (!channel) return
  only.splice(destination, 0, channel)
  emit('update:modelValue', { ...props.modelValue, only })
}
async function probe() {
  const model = props.modelName.trim()
  if (!props.providerId || !model || probing.value) return
  const providerId = props.providerId
  probing.value = true
  try {
    const result = await testModel({ provider_id: providerId, model_name: model, clinepass_probe: true })
    if (props.providerId !== providerId || props.modelName.trim() !== model) return
    if (!result.success || !result.clinepass_probe) throw new Error(result.error || '渠道商探测失败')
    emit('update:modelValue', { ...props.modelValue, ...result.clinepass_probe })
  } catch (error) {
    showError(parseApiError(error, '渠道商探测失败'))
  } finally {
    probing.value = false
  }
}
</script>
