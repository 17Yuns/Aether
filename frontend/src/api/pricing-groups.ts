import apiClient from './client'
import type { TieredPricingConfig } from './endpoints/types'

export interface PricingGroupModelAccess {
  global_model_id: string
  provider_ids: string[]
}

export interface PricingGroup {
  id: string
  name: string
  multiplier: number
  is_visible: boolean
  model_access?: PricingGroupModelAccess[] | null
}

export interface ModelSalePricing {
  default_tiered_pricing?: TieredPricingConfig | null
  default_price_per_request?: number | null
  config?: Record<string, unknown> | null
}

export interface ModelGroupPrice extends ModelSalePricing {
  id: string
  name: string
  multiplier: number
  is_visible: boolean
  is_available: boolean
}

export interface PricingGroupsConfig {
  enabled: boolean
  default_group_id: string
  groups: PricingGroup[]
}

export const pricingGroupsApi = {
  async getAdmin(): Promise<PricingGroupsConfig> {
    return (await apiClient.get<PricingGroupsConfig>('/api/admin/billing/pricing-groups')).data
  },
  async save(config: PricingGroupsConfig): Promise<PricingGroupsConfig> {
    return (await apiClient.put<PricingGroupsConfig>('/api/admin/billing/pricing-groups', config)).data
  },
  async getVisible(): Promise<PricingGroupsConfig> {
    return (await apiClient.get<PricingGroupsConfig>('/api/users/me/pricing-groups')).data
  },
  async getModelPrices(modelId: string): Promise<{ enabled: boolean; group_prices: ModelGroupPrice[]; base_pricing: ModelSalePricing }> {
    return (await apiClient.get<{ enabled: boolean; group_prices: ModelGroupPrice[]; base_pricing: ModelSalePricing }>('/api/admin/billing/pricing-groups', { params: { global_model_id: modelId } })).data
  },
}
