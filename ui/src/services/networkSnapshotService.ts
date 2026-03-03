import type { DataResponse } from '../types'

export async function fetchNetworkSnapshot(networkId: number): Promise<DataResponse> {
  const res = await fetch(`/api/${networkId}/data.json`)
  if (!res.ok) throw new Error(`fetchNetworkSnapshot: ${res.status}`)
  return res.json()
}
