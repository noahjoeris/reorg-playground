import { useCallback, useMemo, useState } from 'react'
import { mutate } from 'swr'
import useSWRMutation from 'swr/mutation'
import { mineBlock } from '../services/miningService'
import { getNetworkSnapshotKey } from '../services/swrKeys'
import type { MineBlockResponse, Network, NodeInfo } from '../types'

type MineControlNode = Pick<NodeInfo, 'id' | 'supports_controls'>
type IsEnabledByNodeId = Record<number, boolean>
type MineBlockMutationArgs = {
  networkId: number
  nodeId: number
  count?: number
}

const MINE_BLOCK_MUTATION_KEY = 'mine-block'

export function useMineBlock(network: Network, nodes: MineControlNode[] = []) {
  const [error, setError] = useState<string | null>(null)
  const { trigger: triggerMineBlock, isMutating } = useSWRMutation<
    MineBlockResponse,
    Error,
    string,
    MineBlockMutationArgs
  >(MINE_BLOCK_MUTATION_KEY, async (_key, { arg }) => {
    const result = await mineBlock(arg.networkId, arg.nodeId, arg.count)
    if (!result.success) {
      throw new Error(result.error ?? 'Unknown error')
    }
    return result
  })
  const isEnabledByNodeId = useMemo(() => {
    const map: IsEnabledByNodeId = {}
    for (const node of nodes) {
      map[node.id] = node.supports_controls
    }
    return map
  }, [nodes])
  const isFeatureEnabled = Object.values(isEnabledByNodeId).some(Boolean)

  const mine = useCallback(
    async (node: MineControlNode, count?: number) => {
      if (!(isEnabledByNodeId[node.id] ?? false)) {
        setError('Mining control is disabled for this network')
        return
      }

      setError(null)
      try {
        await triggerMineBlock({ networkId: network.id, nodeId: node.id, count })
        void mutate(getNetworkSnapshotKey(network.id))
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Network error')
      }
    },
    [isEnabledByNodeId, network, triggerMineBlock],
  )

  return { mine, loading: isMutating, error, isFeatureEnabled, isEnabledByNodeId }
}
