import type { DataResponse, MineBlockResponse, Network, NetworksResponse } from './types'

export async function fetchNetworks(): Promise<Network[]> {
  const res = await fetch('api/networks.json')
  if (!res.ok) throw new Error(`fetchNetworks: ${res.status}`)
  const data: NetworksResponse = await res.json()
  return data.networks
}

export async function fetchData(networkId: number): Promise<DataResponse> {
  const res = await fetch(`api/${networkId}/data.json`)
  if (!res.ok) throw new Error(`fetchData: ${res.status}`)
  return res.json()
}

export function createChangesEventSource(): EventSource {
  return new EventSource('api/changes')
}

export async function mineBlock(networkId: number, nodeId: number): Promise<MineBlockResponse> {
  const res = await fetch(`api/${networkId}/mine-block`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ node_id: nodeId }),
  })
  return res.json()
}
