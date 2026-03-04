import type { DataResponse } from '../types'

export async function fetchNetworkSnapshot(networkId: number, signal?: AbortSignal): Promise<DataResponse> {
  const res = await fetch(`/api/${networkId}/data.json`, { signal })
  if (!res.ok) throw new Error(`fetchNetworkSnapshot: ${res.status}`)
  return res.json()
}
