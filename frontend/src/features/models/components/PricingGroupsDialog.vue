<template>
  <Dialog
    :model-value="open"
    title="分组定价"
    description="模型基础售价 × 分组倍率，即为用户实际价格。"
    size="xl"
    @update:model-value="emit('update:open', $event)"
  >
    <div
      v-if="config"
      class="space-y-4"
    >
      <div class="flex items-center justify-between gap-3">
        <Label for="group-pricing-enabled">启用分组定价</Label>
        <Switch
          id="group-pricing-enabled"
          v-model="config.enabled"
        />
      </div>
      <p class="text-xs text-muted-foreground">
        启用后，用户按 API Key 的分组计费。未选择分组的旧密钥使用默认分组。
      </p>
      <div class="space-y-2">
        <Label for="default-pricing-group">默认分组</Label>
        <select
          id="default-pricing-group"
          v-model="config.default_group_id"
          class="w-full rounded-md border bg-background p-2"
        >
          <option
            v-for="group in config.groups.filter(item => item.is_visible)"
            :key="group.id"
            :value="group.id"
          >
            {{ group.name }}
          </option>
        </select>
      </div>
      <div
        v-for="(group, index) in config.groups"
        :key="index"
        class="grid grid-cols-2 sm:grid-cols-[1fr_1fr_100px_72px_32px] items-end gap-2 rounded-lg border p-3"
      >
        <div class="space-y-1">
          <Label :for="`group-id-${index}`">分组 ID</Label><Input
            :id="`group-id-${index}`"
            v-model="group.id"
            maxlength="100"
          />
        </div>
        <div class="space-y-1">
          <Label :for="`group-name-${index}`">名称</Label><Input
            :id="`group-name-${index}`"
            v-model="group.name"
            maxlength="100"
          />
        </div>
        <div class="space-y-1">
          <Label :for="`group-ratio-${index}`">倍率</Label><Input
            :id="`group-ratio-${index}`"
            :model-value="group.multiplier"
            type="number"
            min="0"
            step="any"
            @update:model-value="group.multiplier = String($event).trim() === '' ? Number.NaN : Number($event)"
          />
        </div>
        <div class="space-y-2 pb-2">
          <Label :for="`group-visible-${index}`">用户可见</Label><Switch
            :id="`group-visible-${index}`"
            v-model="group.is_visible"
          />
        </div>
        <Button
          variant="ghost"
          size="icon"
          :disabled="config.groups.length === 1 || config.default_group_id === group.id"
          :aria-label="`删除 ${group.name}`"
          @click="config.groups.splice(index, 1)"
        >
          <Trash2 class="h-4 w-4" />
        </Button>
      </div>
      <Button
        variant="outline"
        :disabled="config.groups.length >= 100"
        @click="addGroup"
      >
        <Plus class="mr-1 h-4 w-4" />添加分组
      </Button>
      <p class="text-xs text-muted-foreground">
        隐藏后用户无法新选该分组，已有密钥仍按原分组计费。删除或修改分组 ID 前，请先调整使用该分组的密钥。
      </p>
    </div>
    <p
      v-else
      class="py-8 text-center text-muted-foreground"
    >
      {{ loading ? '加载中…' : '配置加载失败，请重新打开重试' }}
    </p>
    <template #footer>
      <Button
        variant="outline"
        :disabled="saving"
        @click="emit('update:open', false)"
      >
        取消
      </Button>
      <Button
        :disabled="!config || loading || saving"
        @click="save"
      >
        {{ saving ? '保存中…' : '保存' }}
      </Button>
    </template>
  </Dialog>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'
import { Plus, Trash2 } from 'lucide-vue-next'
import { Dialog, Button, Input, Label, Switch } from '@/components/ui'
import { pricingGroupsApi, type PricingGroupsConfig } from '@/api/pricing-groups'
import { useToast } from '@/composables/useToast'
import { parseApiError } from '@/utils/errorParser'

const props = defineProps<{ open: boolean }>()
const emit = defineEmits<{ 'update:open': [value: boolean] }>()
const config = ref<PricingGroupsConfig | null>(null)
const loading = ref(false)
const saving = ref(false)
const { success, error } = useToast()
let loadVersion = 0
watch(() => props.open, async open => {
  const version = ++loadVersion
  if (!open) return
  loading.value = true
  config.value = null
  try {
    const result = await pricingGroupsApi.getAdmin()
    if (version === loadVersion) config.value = result
  } catch (reason) {
    if (version === loadVersion) error(parseApiError(reason, '分组定价加载失败'))
  } finally {
    if (version === loadVersion) loading.value = false
  }
})
function addGroup() {
  config.value?.groups.push({ id: `group_${Date.now()}`, name: '新分组', multiplier: 1, is_visible: true })
}
async function save() {
  if (!config.value || saving.value) return
  if (config.value.groups.some(group => !group.id.trim() || !group.name.trim() || !Number.isFinite(group.multiplier) || group.multiplier < 0)) {
    error('请填写分组 ID、名称及大于等于 0 的倍率')
    return
  }
  saving.value = true
  try {
    await pricingGroupsApi.save(config.value)
    success('分组定价已保存')
    emit('update:open', false)
  } catch (reason) {
    error(parseApiError(reason, '保存分组定价失败'))
  } finally {
    saving.value = false
  }
}
</script>
