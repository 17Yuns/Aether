<template>
  <div
    class="space-y-3 rounded-lg border p-3"
    data-testid="clinepass-channel-filters"
  >
    <div class="space-y-1">
      <Label>渠道商筛选</Label>
      <p class="text-xs text-muted-foreground">
        按模型选择允许或排除的渠道商，也可在添加、编辑模型时配置。
      </p>
    </div>
    <div class="flex gap-2">
      <Input
        v-model="newModel"
        placeholder="如 cline-pass/deepseek-v4.1-flash"
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
      class="space-y-2"
    >
      <div class="flex items-center justify-between gap-2">
        <span
          class="min-w-0 truncate text-sm"
          :title="String(model)"
        >{{ model }}</span>
        <Button
          type="button"
          size="sm"
          variant="ghost"
          @click="removeModel(String(model))"
        >
          移除
        </Button>
      </div>
      <ClinePassModelChannelFilter
        :model-value="rule"
        :model-name="String(model)"
        :provider-id="providerId"
        :show-heading="false"
        @update:model-value="update(String(model), $event)"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { Button, Input, Label } from '@/components/ui'
import type { ClinePassConfig, ClinePassModelFilter } from '@/api/endpoints'
import ClinePassModelChannelFilter from './ClinePassModelChannelFilter.vue'
import { useToast } from '@/composables/useToast'

const props = defineProps<{ modelValue: ClinePassConfig; providerId?: string }>()
const emit = defineEmits<{ 'update:modelValue': [value: ClinePassConfig] }>()
const newModel = ref('')
const { error: showError } = useToast()

function update(model: string, rule: ClinePassModelFilter) {
  emit('update:modelValue', { models: { ...props.modelValue.models, [model]: rule } })
}
function addModel() {
  const model = newModel.value.trim()
  if (!model || props.modelValue.models[model]) return
  if (!model.startsWith('cline-pass/') || model === 'cline-pass/') {
    showError('ClinePass 套餐模型必须使用 cline-pass/ 开头的模型 ID')
    return
  }
  update(model, { only: [], exclude: [], available_channels: [] })
  newModel.value = ''
}
function removeModel(model: string) {
  const models = { ...props.modelValue.models }
  delete models[model]
  emit('update:modelValue', { models })
}
</script>
