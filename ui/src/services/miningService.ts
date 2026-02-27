import type { MineBlockResponse } from '../types'

export async function mineBlock(networkId: number, nodeId: number, count?: number): Promise<MineBlockResponse> {
  const res = await fetch(`api/${networkId}/mine-block`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(count === undefined ? { node_id: nodeId } : { node_id: nodeId, count }),
  })
  return res.json()
}
