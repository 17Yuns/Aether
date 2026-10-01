import apiClient from './client'

export interface PricingGroup {
  id: string
  name: string
  multiplier: number
  is_visible: boolean
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
}
