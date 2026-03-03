import type { SetNodeP2PConnectionResponse } from '../types'

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
