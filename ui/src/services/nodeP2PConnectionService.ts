import type { SetNodeP2PConnectionResponse } from '../types'

export type NodeP2PState = {
  node_id: number
  active: boolean | null
}

export type NodeP2PStateResponse = {
  nodes: NodeP2PState[]
}

export async function fetchNodeP2PState(networkId: number, signal?: AbortSignal): Promise<NodeP2PStateResponse> {
  const res = await fetch(`/api/${networkId}/p2p-state.json`, { signal })
  if (!res.ok) throw new Error(`fetchNodeP2PState: ${res.status}`)
  return res.json()
}

export async function setNodeP2PConnectionActive(
  networkId: number,
  nodeId: number,
  active: boolean,
): Promise<SetNodeP2PConnectionResponse> {
  const res = await fetch(`/api/${networkId}/network-active`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ node_id: nodeId, active }),
  })
  return res.json()
}
