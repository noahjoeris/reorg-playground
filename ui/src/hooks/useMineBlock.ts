import { useCallback, useMemo } from 'react'
import { mutate } from 'swr'
import useSWRMutation from 'swr/mutation'
import { mineBlock } from '../services/miningService'
import { getNetworkSnapshotKey } from '../services/swrKeys'
import type { MineBlockResponse, Network, NodeInfo } from '../types'
import { useNotification } from './useNotification'

type MineControlNode = Pick<NodeInfo, 'id' | 'name' | 'supports_mining'>
type IsEnabledByNodeId = Record<number, boolean>
type MineBlockMutationArgs = {
  networkId: number
  nodeId: number
  count?: number
}

const MINE_BLOCK_MUTATION_KEY = 'mine-block'

export function useMineBlock(network: Network, nodes: MineControlNode[] = []) {
  const { notifyError, notifySuccess } = useNotification()
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
      map[node.id] = node.supports_mining
    }
    return map
  }, [nodes])
  const isFeatureEnabled = Object.values(isEnabledByNodeId).some(Boolean)

  const mine = useCallback(
    async (node: MineControlNode, count?: number) => {
      if (!(isEnabledByNodeId[node.id] ?? false)) {
        notifyError({
          title: 'Mining is unavailable',
          description: `Mining control is disabled for ${node.name}.`,
        })
        return
      }

      try {
        await triggerMineBlock({ networkId: network.id, nodeId: node.id, count })
        void mutate(getNetworkSnapshotKey(network.id))

        const minedBlockCount = count ?? 1
        notifySuccess({
          title: minedBlockCount === 1 ? 'Mined 1 block' : `Mined ${minedBlockCount} blocks`,
          description: `Triggered mining on ${node.name}.`,
        })
      } catch (err) {
        notifyError({
          title: 'Could not mine block',
          description: err instanceof Error ? err.message : 'Network error',
        })
      }
    },
    [isEnabledByNodeId, network, notifyError, notifySuccess, triggerMineBlock],
  )

  return { mine, loading: isMutating, isFeatureEnabled, isEnabledByNodeId }
}
