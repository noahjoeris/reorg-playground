import { useCallback, useEffect, useMemo, useState } from 'react'
import { mutate } from 'swr'
import useSWRMutation from 'swr/mutation'
import { setNodeP2PConnectionActive } from '../services/nodeP2PConnectionService'
import { getNetworkSnapshotKey } from '../services/swrKeys'
import type { Network, NodeInfo, SetNodeP2PConnectionResponse } from '../types'
import { isRegtestOrSignet } from '../utils'

type LoadingByNodeId = Record<number, boolean>
type ErrorByNodeId = Record<number, string | null>
type P2PConnectionActiveByNodeId = Record<number, boolean>
type IsEnabledByNodeId = Record<number, boolean>
type SetNodeP2PConnectionArgs = {
  networkId: number
  nodeId: number
  active: boolean
}

const SET_NODE_P2P_CONNECTION_MUTATION_KEY = 'set-node-p2p-connection'

function supportsNodeP2PControl(node: NodeInfo): boolean {
  return node.implementation === 'Bitcoin Core'
}

export function useNodeP2PConnection(network: Network | null, nodes: NodeInfo[] = []) {
  const [loadingByNodeId, setLoadingByNodeId] = useState<LoadingByNodeId>({})
  const [errorByNodeId, setErrorByNodeId] = useState<ErrorByNodeId>({})
  const [p2pConnectionActiveByNodeId, setP2PConnectionActiveByNodeId] = useState<P2PConnectionActiveByNodeId>({})
  const { trigger: triggerSetNodeP2PConnection } = useSWRMutation<
    SetNodeP2PConnectionResponse,
    Error,
    string,
    SetNodeP2PConnectionArgs
  >(SET_NODE_P2P_CONNECTION_MUTATION_KEY, async (_key, { arg }) => {
    const result = await setNodeP2PConnectionActive(arg.networkId, arg.nodeId, arg.active)
    if (!result.success) {
      throw new Error(result.error ?? 'Unknown error')
    }
    return result
  })
  const nodeControlsEnabled = isRegtestOrSignet(network) && !network?.disable_node_controls
  const isEnabledByNodeId = useMemo(() => {
    const map: IsEnabledByNodeId = {}
    for (const node of nodes) {
      map[node.id] = nodeControlsEnabled && supportsNodeP2PControl(node)
    }
    return map
  }, [nodes, nodeControlsEnabled])
  const isFeatureEnabled = Object.values(isEnabledByNodeId).some(Boolean)

  useEffect(() => {
    setLoadingByNodeId({})
    setErrorByNodeId({})
    setP2PConnectionActiveByNodeId({})
  }, [network?.id])

  const getNodeP2PConnectionActive = useCallback(
    (nodeId: number) => {
      return p2pConnectionActiveByNodeId[nodeId] ?? true
    },
    [p2pConnectionActiveByNodeId],
  )

  const setP2PConnectionActive = useCallback(
    async (node: NodeInfo, active: boolean) => {
      const nodeId = node.id
      if (!network) {
        setErrorByNodeId(current => ({ ...current, [nodeId]: 'No network selected' }))
        return false
      }
      if (!(isEnabledByNodeId[nodeId] ?? false)) {
        setErrorByNodeId(current => ({ ...current, [nodeId]: 'P2P control is disabled for this network' }))
        return false
      }

      setLoadingByNodeId(current => ({ ...current, [nodeId]: true }))
      setErrorByNodeId(current => ({ ...current, [nodeId]: null }))

      try {
        await triggerSetNodeP2PConnection({ networkId: network.id, nodeId, active })
        void mutate(getNetworkSnapshotKey(network.id))
        return true
      } catch (error) {
        setErrorByNodeId(current => ({
          ...current,
          [nodeId]: error instanceof Error ? error.message : 'Network error',
        }))
        return false
      } finally {
        setLoadingByNodeId(current => ({ ...current, [nodeId]: false }))
      }
    },
    [isEnabledByNodeId, network, triggerSetNodeP2PConnection],
  )

  const toggleNodeP2PConnection = useCallback(
    async (node: NodeInfo) => {
      const nextActive = !getNodeP2PConnectionActive(node.id)
      const success = await setP2PConnectionActive(node, nextActive)
      if (success) {
        setP2PConnectionActiveByNodeId(current => ({ ...current, [node.id]: nextActive }))
      }
      return success
    },
    [getNodeP2PConnectionActive, setP2PConnectionActive],
  )

  const clearError = useCallback((nodeId: number) => {
    setErrorByNodeId(current => ({ ...current, [nodeId]: null }))
  }, [])

  return {
    setP2PConnectionActive,
    toggleNodeP2PConnection,
    getNodeP2PConnectionActive,
    isEnabledByNodeId,
    loadingByNodeId,
    errorByNodeId,
    clearError,
    isFeatureEnabled,
  }
}
