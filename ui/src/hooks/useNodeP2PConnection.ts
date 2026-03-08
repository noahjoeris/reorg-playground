import { useCallback, useEffect, useMemo, useState } from 'react'
import useSWR from 'swr'
import useSWRMutation from 'swr/mutation'
import {
  fetchNodeP2PState,
  type NodeP2PStateResponse,
  setNodeP2PConnectionActive,
} from '../services/nodeP2PConnectionService'
import type { Network, NodeInfo, SetNodeP2PConnectionResponse } from '../types'

type LoadingByNodeId = Record<number, boolean>
type ErrorByNodeId = Record<number, string | null>
type IsEnabledByNodeId = Record<number, boolean>
type NodeP2PStateKey = readonly ['node-p2p-state', number]
type SetNodeP2PConnectionArgs = {
  networkId: number
  nodeId: number
  active: boolean
}

const SET_NODE_P2P_CONNECTION_MUTATION_KEY = 'set-node-p2p-connection'
const P2P_STATE_REFRESH_INTERVAL_MS = 30_000

export function useNodeP2PConnection(network: Network | null, nodes: NodeInfo[] = []) {
  const [loadingByNodeId, setLoadingByNodeId] = useState<LoadingByNodeId>({})
  const [errorByNodeId, setErrorByNodeId] = useState<ErrorByNodeId>({})
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
  const isEnabledByNodeId = useMemo(() => {
    const map: IsEnabledByNodeId = {}
    for (const node of nodes) {
      map[node.id] = node.supports_controls
    }
    return map
  }, [nodes])
  const isFeatureEnabled = Object.values(isEnabledByNodeId).some(Boolean)
  const { data: p2pStateData, mutate: revalidateP2PState } = useSWR<
    NodeP2PStateResponse,
    Error,
    NodeP2PStateKey | null
  >(
    network && isFeatureEnabled ? ['node-p2p-state', network.id] : null,
    ([, networkId]) => fetchNodeP2PState(networkId),
    {
      revalidateOnFocus: true,
      revalidateOnReconnect: true,
      refreshInterval: P2P_STATE_REFRESH_INTERVAL_MS,
      keepPreviousData: false,
    },
  )

  useEffect(() => {
    setLoadingByNodeId({})
    setErrorByNodeId({})
  }, [network?.id])

  const getNodeP2PConnectionActive = useCallback(
    (nodeId: number) => {
      return p2pStateData?.nodes.find(nodeState => nodeState.node_id === nodeId)?.active
    },
    [p2pStateData],
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
        void revalidateP2PState()
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
    [isEnabledByNodeId, network, revalidateP2PState, triggerSetNodeP2PConnection],
  )

  const toggleNodeP2PConnection = useCallback(
    async (node: NodeInfo) => {
      const currentActive = getNodeP2PConnectionActive(node.id)
      if (currentActive == null) {
        setErrorByNodeId(current => ({ ...current, [node.id]: 'P2P status is still loading' }))
        return false
      }

      return setP2PConnectionActive(node, !currentActive)
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
