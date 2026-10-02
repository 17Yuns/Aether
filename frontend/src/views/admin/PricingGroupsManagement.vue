<template>
  <div class="space-y-5 pb-8">
    <Card class="p-4 sm:p-6">
      <div class="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 class="text-lg font-semibold">
            分组管理
          </h1>
          <p class="mt-1 text-sm text-muted-foreground">
            每组统一倍率，按模型选择允许调用的供应商。API Key 使用所选分组的调用范围和售价。
          </p>
        </div>
        <div class="flex items-center gap-2">
          <Button
            variant="outline"
            :disabled="loading || saving"
            @click="load"
          >
            刷新
          </Button>
          <Button
            :disabled="!config || loading || saving || !dirty"
            @click="save"
          >
            {{ saving ? '保存中…' : '保存配置' }}
          </Button>
        </div>
      </div>
      <div
        v-if="config"
        class="mt-4 flex flex-wrap items-center gap-5 border-t pt-4"
      >
        <div class="flex items-center gap-2">
          <Switch
            id="pricing-groups-enabled"
            v-model="config.enabled"
          />
          <Label for="pricing-groups-enabled">启用分组定价与调用范围</Label>
        </div>
        <div class="flex items-center gap-2">
          <Label for="default-pricing-group">默认分组</Label>
          <select
            id="default-pricing-group"
            v-model="config.default_group_id"
            class="rounded-md border bg-background px-3 py-2 text-sm"
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
        <span
          v-if="dirty"
          class="text-xs text-amber-600"
        >有未保存的修改</span>
      </div>
    </Card>
    <LoadingState
      v-if="loading"
      message="加载分组和模型…"
    />
    <Card
      v-else-if="!config"
      class="p-8 text-center text-muted-foreground"
    >
      配置加载失败，请刷新重试。
    </Card>
    <div
      v-else
      class="grid items-start gap-5 lg:grid-cols-[280px_minmax(0,1fr)]"
    >
      <Card class="overflow-hidden">
        <div class="flex items-center justify-between border-b p-4">
          <h2 class="font-semibold">
            全部分组 <span class="text-muted-foreground">{{ config.groups.length }}</span>
          </h2>
          <Button
            variant="outline"
            size="sm"
            :disabled="config.groups.length >= 100"
            @click="addGroup"
          >
            <Plus class="mr-1 h-4 w-4" />添加
          </Button>
        </div>
        <button
          v-for="group in config.groups"
          :key="group.id"
          type="button"
          :aria-label="`编辑分组 ${group.name}`"
          class="block w-full border-b p-4 text-left transition-colors last:border-0 hover:bg-muted/40"
          :class="selectedId === group.id ? 'bg-primary/5 ring-1 ring-inset ring-primary/30' : ''"
          @click="selectedId = group.id"
        >
          <div class="flex items-center justify-between gap-2">
            <span class="truncate font-medium">{{ group.name || '未命名分组' }}</span>
            <span class="shrink-0 font-mono text-primary">{{ group.multiplier }}×</span>
          </div>
          <div class="mt-2 flex flex-wrap gap-1">
            <Badge
              v-if="config.default_group_id === group.id"
              variant="secondary"
            >
              默认
            </Badge>
            <Badge variant="secondary">
              {{ group.is_visible ? '用户可见' : '隐藏' }}
            </Badge>
            <Badge variant="secondary">
              {{ group.model_access == null ? '全部模型 / 供应商' : `${group.model_access.length} 个模型 · ${providerCount(group)} 个供应商` }}
            </Badge>
          </div>
          <p
            v-if="group.model_access"
            class="mt-2 line-clamp-2 text-xs text-muted-foreground"
          >
            {{ group.model_access.map(model => modelName(model.global_model_id)).join('、') || '尚未开放模型' }}
          </p>
        </button>
      </Card>
      <div
        v-if="selectedGroup"
        class="min-w-0 space-y-5"
      >
        <Card class="p-4 sm:p-6">
          <div class="flex items-center justify-between gap-2">
            <h2 class="font-semibold">
              {{ selectedGroup.name || '新分组' }} · 设置
            </h2>
            <Button
              variant="ghost"
              size="sm"
              :disabled="config.default_group_id === selectedGroup.id || config.groups.length === 1"
              @click="removeGroup"
            >
              <Trash2 class="mr-1 h-4 w-4" />删除分组
            </Button>
          </div>
          <div class="mt-4 grid gap-4 sm:grid-cols-3">
            <div class="space-y-2">
              <Label for="pricing-group-name">分组名称</Label><Input
                id="pricing-group-name"
                v-model="selectedGroup.name"
                maxlength="100"
              />
            </div>
            <div class="space-y-2">
              <Label for="pricing-group-id">分组 ID</Label><Input
                id="pricing-group-id"
                :model-value="selectedGroup.id"
                disabled
              /><p class="text-xs text-muted-foreground">
                用于 API Key 的分组选择
              </p>
            </div>
            <div class="space-y-2">
              <Label for="pricing-group-multiplier">统一倍率</Label><Input
                id="pricing-group-multiplier"
                :model-value="selectedGroup.multiplier"
                type="number"
                min="0"
                step="any"
                @update:model-value="selectedGroup.multiplier = String($event).trim() === '' ? Number.NaN : Number($event)"
              /><p class="text-xs text-muted-foreground">
                组内所有模型售价 = 基础售价 × 此倍率
              </p>
            </div>
          </div>
          <div class="mt-4 flex items-center gap-2">
            <Switch
              id="pricing-group-visible"
              v-model="selectedGroup.is_visible"
              :disabled="selectedGroup.id === config.default_group_id"
            />
            <Label for="pricing-group-visible">用户可见，可选择此分组并查看价格</Label>
          </div>
        </Card>
        <Card class="p-4 sm:p-6">
          <div class="flex flex-wrap items-center justify-between gap-3">
            <div>
              <h2 class="font-semibold">
                调用范围总览
              </h2><p class="mt-1 text-xs text-muted-foreground">
                同一供应商的同一模型可以加入多个分组，各组独立计费。
              </p>
            </div>
            <label class="flex items-center gap-2 text-sm"><input
              type="checkbox"
              :checked="selectedGroup.model_access != null"
              @change="toggleRestriction(($event.target as HTMLInputElement).checked)"
            >仅允许选中的模型和供应商</label>
          </div>
          <p
            v-if="selectedGroup.model_access == null"
            class="mt-4 rounded-md bg-muted/40 p-4 text-sm"
          >
            此分组允许所有模型和供应商。勾选右上方选项后，可按模型限制供应商。
          </p>
          <div
            v-else
            class="mt-4 overflow-x-auto"
          >
            <table class="w-full text-sm">
              <thead>
                <tr class="border-b text-left text-muted-foreground">
                  <th class="py-2 pr-4 font-medium">
                    模型
                  </th><th class="py-2 font-medium">
                    允许的供应商
                  </th><th class="w-16" />
                </tr>
              </thead>
              <tbody>
                <tr
                  v-for="entry in selectedGroup.model_access"
                  :key="entry.global_model_id"
                  class="border-b last:border-0"
                >
                  <td class="py-3 pr-4 font-mono">
                    <button
                      class="text-left hover:text-primary"
                      @click="expandModel(entry.global_model_id)"
                    >
                      {{ modelName(entry.global_model_id) }}
                    </button>
                  </td>
                  <td class="py-3">
                    <div class="flex flex-wrap gap-1">
                      <Badge
                        v-for="id in entry.provider_ids"
                        :key="id"
                        variant="secondary"
                      >
                        {{ providerName(id) }}
                      </Badge>
                    </div>
                  </td>
                  <td class="py-3 text-right">
                    <Button
                      variant="ghost"
                      size="icon"
                      :aria-label="`移除 ${modelName(entry.global_model_id)}`"
                      @click="removeModel(entry.global_model_id)"
                    >
                      <X class="h-4 w-4" />
                    </Button>
                  </td>
                </tr>
                <tr v-if="!selectedGroup.model_access.length">
                  <td
                    colspan="3"
                    class="py-6 text-center text-muted-foreground"
                  >
                    尚未开放模型，请在下方展开模型并选择供应商。
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
        </Card>
        <Card
          v-if="selectedGroup.model_access != null"
          class="overflow-hidden"
        >
          <div class="flex flex-wrap items-center justify-between gap-3 border-b p-4">
            <div>
              <h2 class="font-semibold">
                添加模型与供应商
              </h2><p class="mt-1 text-xs text-muted-foreground">
                先展开模型，再选择供应商；不会自动加入该模型的全部渠道。
              </p>
            </div>
            <Input
              v-model="search"
              class="w-full sm:w-64"
              placeholder="搜索模型名称…"
              aria-label="搜索分组模型"
            />
          </div>
          <div class="max-h-[650px] overflow-y-auto divide-y">
            <div
              v-for="model in filteredModels"
              :key="model.id"
            >
              <button
                type="button"
                class="flex w-full items-center justify-between gap-3 p-4 text-left hover:bg-muted/30"
                :aria-label="`展开模型 ${model.name}`"
                :aria-expanded="expanded.has(model.id)"
                @click="expandModel(model.id)"
              >
                <div class="min-w-0">
                  <span class="font-medium">{{ model.name }}</span><span
                    v-if="model.display_name && model.display_name !== model.name"
                    class="ml-2 text-xs text-muted-foreground"
                  >{{ model.display_name }}</span>
                </div>
                <div class="flex shrink-0 items-center gap-2">
                  <Badge
                    v-if="accessFor(model.id)"
                    variant="secondary"
                  >
                    已选 {{ accessFor(model.id)?.provider_ids.length }} 个供应商
                  </Badge><ChevronDown
                    class="h-4 w-4 transition-transform"
                    :class="expanded.has(model.id) ? 'rotate-180' : ''"
                  />
                </div>
              </button>
              <div
                v-if="expanded.has(model.id)"
                class="space-y-3 bg-muted/20 px-4 pb-4"
              >
                <p
                  v-if="providerLoading[model.id]"
                  class="py-3 text-sm text-muted-foreground"
                >
                  加载供应商…
                </p>
                <div
                  v-else-if="providerErrors[model.id]"
                  class="flex items-center gap-3 py-3 text-sm"
                >
                  <span class="text-destructive">{{ providerErrors[model.id] }}</span><Button
                    variant="outline"
                    size="sm"
                    @click="loadModelProviders(model.id)"
                  >
                    重试
                  </Button>
                </div>
                <template v-else>
                  <div class="flex flex-wrap items-center gap-2">
                    <Input
                      v-model="providerSearch[model.id]"
                      class="h-8 w-48"
                      placeholder="筛选供应商…"
                      :aria-label="`筛选 ${model.name} 的供应商`"
                    />
                    <Button
                      variant="outline"
                      size="sm"
                      :disabled="!modelProviders[model.id]?.length"
                      @click="selectAllProviders(model.id)"
                    >
                      选择此模型的全部供应商
                    </Button>
                    <Button
                      variant="ghost"
                      size="sm"
                      @click="removeModel(model.id)"
                    >
                      清空选择
                    </Button>
                  </div>
                  <div class="grid gap-2 sm:grid-cols-2 xl:grid-cols-3">
                    <label
                      v-for="provider in filteredProviders(model.id)"
                      :key="provider.provider_id"
                      class="flex cursor-pointer items-start gap-2 rounded-md border bg-background p-3"
                      :class="accessFor(model.id)?.provider_ids.includes(provider.provider_id) ? 'border-primary/50' : ''"
                    >
                      <input
                        type="checkbox"
                        class="mt-1"
                        :aria-label="`${model.name} · ${provider.provider_name}`"
                        :checked="accessFor(model.id)?.provider_ids.includes(provider.provider_id) || false"
                        @change="selectProvider(model.id, provider.provider_id, ($event.target as HTMLInputElement).checked)"
                      >
                      <div class="min-w-0"><div class="text-sm font-medium">{{ provider.provider_name }} <span
                        v-if="!provider.is_active"
                        class="text-xs text-muted-foreground"
                      >（停用）</span></div><div class="mt-1 break-all font-mono text-xs text-muted-foreground">{{ provider.target_model }}</div></div>
                    </label>
                  </div>
                  <p
                    v-if="!filteredProviders(model.id).length"
                    class="py-3 text-sm text-muted-foreground"
                  >
                    没有匹配的关联供应商。
                  </p>
                </template>
              </div>
            </div>
            <p
              v-if="!filteredModels.length"
              class="p-8 text-center text-muted-foreground"
            >
              没有匹配的模型。
            </p>
          </div>
        </Card>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { Plus, Trash2, X, ChevronDown } from 'lucide-vue-next'
import { Badge, Button, Card, Input, Label, Switch } from '@/components/ui'
import { LoadingState } from '@/components/common'
import { pricingGroupsApi, type PricingGroup, type PricingGroupModelAccess, type PricingGroupsConfig } from '@/api/pricing-groups'
import { getGlobalModels, getGlobalModelProviders } from '@/api/global-models'
import { getProvidersSummary } from '@/api/endpoints/providers'
import type { GlobalModelResponse, ModelCatalogProviderDetail, ProviderWithEndpointsSummary } from '@/api/endpoints/types'
import { useToast } from '@/composables/useToast'
import { parseApiError } from '@/utils/errorParser'

const config = ref<PricingGroupsConfig | null>(null)
const saved = ref('')
const loading = ref(false)
const saving = ref(false)
const selectedId = ref('')
const models = ref<GlobalModelResponse[]>([])
const providers = ref<ProviderWithEndpointsSummary[]>([])
const modelProviders = ref<Record<string, ModelCatalogProviderDetail[]>>({})
const providerLoading = ref<Record<string, boolean>>({})
const providerErrors = ref<Record<string, string>>({})
const providerSearch = ref<Record<string, string>>({})
const expanded = ref(new Set<string>())
const search = ref('')
const { success, error } = useToast()
const selectedGroup = computed(() => config.value?.groups.find(group => group.id === selectedId.value))
const dirty = computed(() => config.value != null && JSON.stringify(config.value) !== saved.value)
const modelNames = computed(() => new Map(models.value.map(model => [model.id, model.name])))
const providerNames = computed(() => new Map(providers.value.map(provider => [provider.id, provider.name])))
const filteredModels = computed(() => {
  const words = search.value.trim().toLowerCase().split(/\s+/).filter(Boolean)
  return models.value.filter(model => words.every(word => `${model.name} ${model.display_name || ''}`.toLowerCase().includes(word)))
})
function modelName(id: string) { return modelNames.value.get(id) ?? `已移除模型 (${id})` }
function providerName(id: string) { return providerNames.value.get(id) ?? `已移除供应商 (${id})` }
function providerCount(group: PricingGroup) { return new Set(group.model_access?.flatMap(model => model.provider_ids)).size }
function accessFor(id: string) { return selectedGroup.value?.model_access?.find(model => model.global_model_id === id) }
function filteredProviders(id: string) {
  const term = (providerSearch.value[id] || '').toLowerCase().trim()
  return (modelProviders.value[id] || []).filter(provider => `${provider.provider_name} ${provider.target_model}`.toLowerCase().includes(term))
}
async function loadModels() {
  const items: GlobalModelResponse[] = []
  while (true) {
    const page = await getGlobalModels({ skip: items.length, limit: 1000 })
    items.push(...page.models)
    if (items.length >= page.total || !page.models.length) return items
  }
}
async function loadProviders() {
  const items: ProviderWithEndpointsSummary[] = []
  for (let page = 1; ; page++) {
    const result = await getProvidersSummary({ page, page_size: 100 })
    items.push(...result.items)
    if (items.length >= result.total || !result.items.length) return items
  }
}
async function load() {
  if (loading.value || saving.value) return
  if (dirty.value && !window.confirm('刷新会丢弃未保存的修改，是否继续？')) return
  loading.value = true
  try {
    const [groups, allModels, allProviders] = await Promise.all([pricingGroupsApi.getAdmin(), loadModels(), loadProviders()])
    config.value = groups
    models.value = allModels
    providers.value = allProviders
    saved.value = JSON.stringify(groups)
    selectedId.value = groups.groups.some(group => group.id === selectedId.value) ? selectedId.value : groups.default_group_id
    modelProviders.value = {}
    expanded.value = new Set()
    restrictedSelections.clear()
  } catch (reason) { error(parseApiError(reason, '加载分组管理失败')) }
  finally { loading.value = false }
}
function addGroup() {
  const group: PricingGroup = { id: `group_${crypto.randomUUID()}`, name: '新分组', multiplier: 1, is_visible: true, model_access: [] }
  config.value?.groups.push(group)
  selectedId.value = group.id
}
function removeGroup() {
  const group = selectedGroup.value
  if (!group || !config.value || group.id === config.value.default_group_id) return
  if (!window.confirm(`删除“${group.name}”后，仍使用该分组的密钥将无法调用。请确认已调整密钥分组。`)) return
  config.value.groups = config.value.groups.filter(item => item.id !== group.id)
  selectedId.value = config.value.default_group_id
}
const restrictedSelections = new Map<string, PricingGroupModelAccess[]>()
function toggleRestriction(restricted: boolean) {
  const group = selectedGroup.value
  if (!group) return
  if (!restricted && group.model_access) restrictedSelections.set(group.id, group.model_access)
  group.model_access = restricted ? restrictedSelections.get(group.id) ?? [] : null
}
async function expandModel(id: string) {
  if (expanded.value.has(id)) { expanded.value.delete(id); return }
  expanded.value.add(id)
  if (!modelProviders.value[id]) await loadModelProviders(id)
}
async function loadModelProviders(id: string) {
  if (providerLoading.value[id]) return
  providerLoading.value[id] = true
  delete providerErrors.value[id]
  try {
    const response = await getGlobalModelProviders(id)
    const unique = new Map<string, ModelCatalogProviderDetail>()
    for (const provider of response.providers) {
      const previous = unique.get(provider.provider_id)
      unique.set(provider.provider_id, previous ? { ...previous, target_model: [...new Set([previous.target_model, provider.target_model])].join('、') } : provider)
    }
    modelProviders.value[id] = [...unique.values()]
  } catch (reason) { providerErrors.value[id] = parseApiError(reason, '供应商加载失败') }
  finally { providerLoading.value[id] = false }
}
function selectProvider(modelId: string, providerId: string, checked: boolean) {
  const group = selectedGroup.value
  if (!group?.model_access) return
  let entry = accessFor(modelId)
  if (checked) {
    if (!entry) { entry = { global_model_id: modelId, provider_ids: [] }; group.model_access.push(entry) }
    if (!entry.provider_ids.includes(providerId)) entry.provider_ids.push(providerId)
  } else if (entry) {
    entry.provider_ids = entry.provider_ids.filter(id => id !== providerId)
    if (!entry.provider_ids.length) removeModel(modelId)
  }
}
function selectAllProviders(modelId: string) {
  for (const provider of modelProviders.value[modelId] || []) selectProvider(modelId, provider.provider_id, true)
}
function removeModel(id: string) {
  if (selectedGroup.value?.model_access) selectedGroup.value.model_access = selectedGroup.value.model_access.filter(model => model.global_model_id !== id)
}
async function save() {
  if (!config.value || saving.value) return
  if (config.value.groups.some(group => !group.name.trim() || !Number.isFinite(group.multiplier) || group.multiplier < 0)) {
    error('请填写分组名称和大于等于 0 的统一倍率'); return
  }
  saving.value = true
  try {
    const result = await pricingGroupsApi.save(config.value)
    config.value = result
    saved.value = JSON.stringify(result)
    success('分组倍率和调用范围已保存')
  } catch (reason) { error(parseApiError(reason, '保存分组配置失败')) }
  finally { saving.value = false }
}
onMounted(load)
</script>
