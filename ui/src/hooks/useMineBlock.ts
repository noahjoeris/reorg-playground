import { useCallback, useMemo, useState } from 'react'
import { mineBlock } from '../services/miningService'
import type { Network, NodeInfo } from '../types'
import { isRegtestOrSignet } from '../utils'

type MineControlNode = Pick<NodeInfo, 'id' | 'implementation'>
type IsEnabledByNodeId = Record<number, boolean>

function supportsNodeMining(node: Pick<NodeInfo, 'implementation'>): boolean {
  return node.implementation === 'Bitcoin Core'
}

export function useMineBlock(network: Network | null, nodes: MineControlNode[] = []) {
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
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

      setLoading(true)
      setError(null)
      try {
        const result = await mineBlock(network.id, node.id, count)
        if (!result.success) {
          setError(result.error ?? 'Unknown error')
        }
      } catch (e) {
        setError(e instanceof Error ? e.message : 'Network error')
      } finally {
        setLoading(false)
      }
    },
    [isEnabledByNodeId, network],
  )

  const clearError = useCallback(() => setError(null), [])

  return { mine, loading, error, clearError, isFeatureEnabled, isEnabledByNodeId }
}
