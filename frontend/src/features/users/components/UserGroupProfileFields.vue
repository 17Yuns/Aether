<template>
  <div class="space-y-4">
    <div class="space-y-2">
      <Label class="text-sm font-medium">{{ legacyT('名称') }}</Label>
      <Input
        :model-value="name"
        class="h-10"
        :placeholder="legacyT('例如：生产团队')"
        @update:model-value="$emit('update:name', $event)"
      />
    </div>

    <div class="grid gap-3 sm:grid-cols-2">
      <div class="space-y-2">
        <Label class="text-sm font-medium">{{ legacyT('倍率优先级') }}</Label>
        <Input
          :model-value="priority"
          class="h-10"
          type="number"
          step="1"
          @update:model-value="updatePriority"
        />
        <p class="text-xs text-muted-foreground">
          {{ legacyT('用户属于多个已配置倍率的分组时，数值较大的分组生效') }}
        </p>
      </div>

      <div class="space-y-2">
        <div class="flex items-center justify-between gap-3">
          <Label class="text-sm font-medium">{{ legacyT('分组计费倍率') }}</Label>
          <Switch
            :model-value="billingMultiplier !== undefined"
            @update:model-value="toggleBillingMultiplier"
          />
        </div>
        <Input
          :model-value="billingMultiplier ?? 1"
          class="h-10"
          type="number"
          min="0"
          max="1000"
          step="0.01"
          :disabled="billingMultiplier === undefined"
          @update:model-value="updateBillingMultiplier"
        />
        <p class="text-xs text-muted-foreground">
          {{ legacyT(billingMultiplier === undefined ? '未配置，继续使用 Key 自身倍率' : '套餐倍率生效时会优先覆盖此倍率') }}
        </p>
      </div>
    </div>

    <div class="space-y-2">
      <Label class="text-sm font-medium">{{ legacyT('成员') }}</Label>
      <MultiSelect
        :model-value="memberUserIds"
        :options="userOptions"
        :search-threshold="0"
        :disabled="membersDisabled"
        :placeholder="legacyT('选择用户')"
        :empty-text="legacyT('暂无用户')"
        :no-results-text="legacyT('未找到匹配用户')"
        @update:model-value="$emit('update:memberUserIds', $event)"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import { Input, Label, Switch } from '@/components/ui'
import { MultiSelect } from '@/components/common'
import { useI18n } from '@/i18n'
import type { UserSelectOption } from './user-management-types'

const props = defineProps<{
  name: string
  priority: number
  billingMultiplier: number | undefined
  memberUserIds: string[]
  userOptions: UserSelectOption[]
  membersDisabled: boolean
}>()

const emit = defineEmits<{
  'update:name': [value: string]
  'update:priority': [value: number]
  'update:billingMultiplier': [value: number | undefined]
  'update:memberUserIds': [value: string[]]
}>()

const { legacyT } = useI18n()

function updatePriority(value: string | number): void {
  const parsed = Number(value)
  emit('update:priority', Number.isFinite(parsed) ? Math.trunc(parsed) : 0)
}

function toggleBillingMultiplier(enabled: boolean): void {
  emit('update:billingMultiplier', enabled ? (props.billingMultiplier ?? 1) : undefined)
}

function updateBillingMultiplier(value: string | number): void {
  const parsed = Number(value)
  emit('update:billingMultiplier', Number.isFinite(parsed) ? parsed : 1)
}
</script>
