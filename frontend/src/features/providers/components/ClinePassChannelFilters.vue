<template>
  <div
    class="space-y-3 rounded-lg border p-3"
    data-testid="clinepass-channel-filters"
  >
    <div class="space-y-1">
      <Label>渠道商筛选</Label>
      <p class="text-xs text-muted-foreground">
        按模型选择允许或排除的渠道商。未配置时由 ClinePass 自动选择。
      </p>
      <p class="text-xs text-muted-foreground">
        探测会发送最多两次模型请求并消耗套餐额度；保存提供商并添加密钥后可使用。
      </p>
    </div>
    <div class="flex gap-2">
      <Input
        v-model="newModel"
        placeholder="上游模型名称"
        aria-label="ClinePass 模型名称"
        @keydown.enter.prevent="addModel"
      />
      <Button
        type="button"
        variant="outline"
        :disabled="!newModel.trim()"
        @click="addModel"
      >
        添加模型
      </Button>
    </div>
    <div
      v-for="(rule, model) in modelValue.models"
      :key="model"
      class="space-y-2 rounded border p-2"
    >
      <div class="flex items-center justify-between gap-2">
        <span
          class="min-w-0 truncate text-sm"
          :title="String(model)"
        >{{ model }}</span>
        <div class="flex shrink-0 gap-1">
          <Button
            type="button"
            size="sm"
            variant="outline"
            :disabled="!providerId || !!probing"
            @click="probe(String(model))"
          >
            {{ probing === model ? '探测中…' : '探测渠道商' }}
          </Button>
          <Button
            type="button"
            size="sm"
            variant="ghost"
            @click="removeModel(String(model))"
          >
            移除
          </Button>
        </div>
      </div>
      <p
        v-if="rule.pinnable === false"
        class="text-xs text-amber-600"
      >
        {{ rule.pin_reason || '该模型不支持渠道商筛选' }}
      </p>
      <template v-else>
        <div class="space-y-1">
          <Label class="text-xs">允许渠道商（逗号分隔，留空表示全部）</Label>
          <Input
            :model-value="drafts[String(model)]?.only ?? rule.only.join(', ')"
            placeholder="例如 baseten, deepseek"
            @update:model-value="setList(String(model), 'only', String($event))"
          />
        </div>
        <div class="space-y-1">
          <Label class="text-xs">排除渠道商（逗号分隔）</Label>
          <Input
            :model-value="drafts[String(model)]?.exclude ?? rule.exclude.join(', ')"
            placeholder="先探测以获取可用渠道商"
            @update:model-value="setList(String(model), 'exclude', String($event))"
          />
        </div>
      </template>
      <div
        v-if="rule.available_channels.length"
        class="text-xs text-muted-foreground"
      >
        可用渠道商: {{ rule.available_channels.join(', ') }}
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'
import { Button, Input, Label } from '@/components/ui'
import { testModel, type ClinePassConfig, type ClinePassModelFilter } from '@/api/endpoints'
import { useToast } from '@/composables/useToast'
import { parseApiError } from '@/utils/errorParser'

const props = defineProps<{ modelValue: ClinePassConfig; providerId?: string }>()
const emit = defineEmits<{ 'update:modelValue': [value: ClinePassConfig] }>()
const newModel = ref('')
const probing = ref<string | null>(null)
const drafts = ref<Record<string, { only: string; exclude: string }>>({})
const { error: showError } = useToast()

watch(() => props.providerId, () => { drafts.value = {} })
watch(() => props.modelValue, (config) => {
  for (const [model, draft] of Object.entries(drafts.value)) {
    const rule = config.models[model]
    if (!rule) { delete drafts.value[model]; continue }
    for (const key of ['only', 'exclude'] as const) {
      if (JSON.stringify(parseChannels(draft[key])) !== JSON.stringify(rule[key])) {
        draft[key] = rule[key].join(', ')
      }
    }
  }
})

function update(model: string, rule: ClinePassModelFilter) {
  emit('update:modelValue', { models: { ...props.modelValue.models, [model]: rule } })
}
function addModel() {
  const model = newModel.value.trim()
  if (!model || props.modelValue.models[model]) return
  update(model, { only: [], exclude: [], available_channels: [] })
  newModel.value = ''
}
function removeModel(model: string) {
  const models = { ...props.modelValue.models }
  delete models[model]
  emit('update:modelValue', { models })
}
function parseChannels(text: string) {
  return [...new Set(text.split(/[,，]/).map(value => value.trim()).filter(Boolean))]
}
function setList(model: string, key: 'only' | 'exclude', text: string) {
  const rule = props.modelValue.models[model]
  if (!rule) return
  drafts.value[model] = { ...(drafts.value[model] ?? { only: rule.only.join(', '), exclude: rule.exclude.join(', ') }), [key]: text }
  const channels = parseChannels(text)
  update(model, { ...rule, [key]: channels })
}
async function probe(model: string) {
  if (!props.providerId) return
  probing.value = model
  try {
    const result = await testModel({ provider_id: props.providerId, model_name: model, clinepass_probe: true })
    if (!result.success || !result.clinepass_probe) throw new Error(result.error || '渠道商探测失败')
    const rule = props.modelValue.models[model]
    if (rule) update(model, { ...rule, ...result.clinepass_probe })
  } catch (error) {
    showError(parseApiError(error, '渠道商探测失败'))
  } finally {
    probing.value = null
  }
}
</script>
