import type { Network, NetworksResponse } from '../types'

export async function fetchNetworks(): Promise<Network[]> {
  const res = await fetch('/api/networks.json')
  if (!res.ok) throw new Error(`fetchNetworks: ${res.status}`)
  const data: NetworksResponse = await res.json()
  return data.networks
}
