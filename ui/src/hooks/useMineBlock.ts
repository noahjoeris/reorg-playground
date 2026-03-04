import { useCallback, useMemo, useState } from 'react'
import { mutate } from 'swr'
import useSWRMutation from 'swr/mutation'
import { mineBlock } from '../services/miningService'
import { getNetworkSnapshotKey } from '../services/swrKeys'
import type { MineBlockResponse, Network, NodeInfo } from '../types'
import { isRegtestOrSignet } from '../utils'

type MineControlNode = Pick<NodeInfo, 'id' | 'implementation'>
type IsEnabledByNodeId = Record<number, boolean>
type MineBlockMutationArgs = {
  networkId: number
  nodeId: number
  count?: number
}

const MINE_BLOCK_MUTATION_KEY = 'mine-block'

function supportsNodeMining(node: Pick<NodeInfo, 'implementation'>): boolean {
  return node.implementation === 'Bitcoin Core'
}

export function useMineBlock(network: Network | null, nodes: MineControlNode[] = []) {
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
  const nodeControlsEnabled = isRegtestOrSignet(network) && !network?.disable_node_controls
  const isEnabledByNodeId = useMemo(() => {
    const map: IsEnabledByNodeId = {}
    for (const node of nodes) {
      map[node.id] = nodeControlsEnabled && supportsNodeMining(node)
    }
    return map
  }, [nodes, nodeControlsEnabled])
  const isFeatureEnabled = Object.values(isEnabledByNodeId).some(Boolean)

  const mine = useCallback(
    async (node: MineControlNode, count?: number) => {
      if (!network) {
        setError('No network selected')
        return
      }
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
