import { useCallback, useEffect, useMemo, useState } from 'react'
import { TooltipProvider } from '@/components/ui/tooltip'
import { useNetworkData } from '@/hooks/useNetworkData'
import { useNetworks } from '@/hooks/useNetworks'
import { useTheme } from '@/hooks/useTheme'
import { AppHeader } from './AppHeader'
import { BlockDetailPanel } from './BlockDetailPanel'
import { BlockGraph } from './BlockGraph'
import { NodeSection } from './NodeSection'
import { buildReactFlowGraph, type FlowNodeType, type FoldMetadata, preprocessData } from './tree'

const EMPTY_FOLD_META: FoldMetadata = {
  potentialFoldedSegmentCount: 0,
  activeFoldedSegmentCount: 0,
  hiddenBlockIds: new Set<number>(),
}

function getNetworkIdFromUrl(): number | null {
  const params = new URLSearchParams(window.location.search)
  const value = params.get('network')
  if (!value) return null

  const networkId = Number(value)
  return Number.isFinite(networkId) ? networkId : null
}

function setNetworkIdInUrl(id: number) {
  const url = new URL(window.location.href)
  url.searchParams.set('network', String(id))
  window.history.replaceState({}, '', url.toString())
}

function App() {
  const { networks, loading: networksLoading, error: networksError } = useNetworks()
  const { preference: themePreference, cycle: cycleTheme } = useTheme()
  const [selectedNetworkId, setSelectedNetworkId] = useState<number | null>(getNetworkIdFromUrl)
  const [selectedBlockId, setSelectedBlockId] = useState<number | null>(null)
  const [globalCollapsed, setGlobalCollapsed] = useState(true)

  useEffect(() => {
    if (selectedNetworkId !== null) return
    if (networks.length === 0) return

    const firstNetworkId = networks[0]?.id
    if (firstNetworkId === undefined) return

    setSelectedNetworkId(firstNetworkId)
    setNetworkIdInUrl(firstNetworkId)
  }, [networks, selectedNetworkId])

  const handleNetworkChange = useCallback((id: number) => {
    setSelectedNetworkId(id)
    setNetworkIdInUrl(id)
    setSelectedBlockId(null)
    setGlobalCollapsed(true)
  }, [])

  const { data, loading: dataLoading, error: dataError, connectionStatus } = useNetworkData(selectedNetworkId)

  const processedBlocks = useMemo(() => {
    return data ? preprocessData(data) : []
  }, [data])

  const selectedBlock = useMemo(() => {
    if (selectedBlockId === null) return null
    return processedBlocks.find(block => block.id === selectedBlockId) ?? null
  }, [processedBlocks, selectedBlockId])

  useEffect(() => {
    if (selectedBlockId !== null && !selectedBlock) {
      setSelectedBlockId(null)
    }
  }, [selectedBlockId, selectedBlock])

  const handleBlockClick = useCallback((blockId: number) => {
    setSelectedBlockId(blockId)
  }, [])

  const selectedNetwork = useMemo(() => {
    return networks.find(n => n.id === selectedNetworkId) ?? null
  }, [networks, selectedNetworkId])

  const { nodes, edges, foldMeta } = useMemo(() => {
    if (processedBlocks.length === 0) {
      return { nodes: [] as FlowNodeType[], edges: [], foldMeta: EMPTY_FOLD_META }
    }

    return buildReactFlowGraph(
      processedBlocks,
      block => handleBlockClick(block.id),
      selectedBlockId,
      selectedNetwork,
      data?.nodes ?? [],
      globalCollapsed,
    )
  }, [processedBlocks, handleBlockClick, selectedBlockId, selectedNetwork, data, globalCollapsed])

  // Clear selection if the selected block is hidden by folding
  useEffect(() => {
    if (selectedBlockId !== null && foldMeta.hiddenBlockIds.has(selectedBlockId)) {
      setSelectedBlockId(null)
    }
  }, [selectedBlockId, foldMeta.hiddenBlockIds])

  if (networksLoading) {
    return (
      <div className="relative isolate flex h-screen items-center justify-center bg-background px-6 text-center">
        <div className="panel-glass-strong rounded-2xl px-6 py-5">
          <p className="text-sm text-muted-foreground">Loading network configuration...</p>
        </div>
      </div>
    )
  }

  if (networksError) {
    return (
      <div className="relative isolate flex h-screen items-center justify-center bg-background px-6">
        <div className="panel-glass-strong max-w-lg rounded-2xl border-destructive/35 bg-destructive/10 p-5 text-sm text-destructive">
          Could not load network list: {networksError}
        </div>
      </div>
    )
  }

  if (networks.length === 0) {
    return (
      <div className="relative isolate flex h-screen items-center justify-center bg-background px-6">
        <div className="panel-glass-strong max-w-lg rounded-2xl p-5 text-sm text-muted-foreground">
          No networks configured.
        </div>
      </div>
    )
  }

  const totalNodes = data?.nodes.length ?? 0
  const reachableNodes = data?.nodes.filter(node => node.reachable).length ?? 0

  function getEmptyState(): { title: string; message: string } | null {
    if (selectedNetworkId === null) {
      return { title: 'Select a network', message: 'Choose a configured network to load blockchain data.' }
    }
    if (dataLoading && !data) {
      return { title: 'Loading chain data', message: 'Fetching latest tips and headers from configured nodes.' }
    }
    if (dataError && !data) {
      return { title: 'Failed to load chain data', message: dataError }
    }
    if (data && processedBlocks.length === 0) {
      return {
        title: 'No blocks to render',
        message:
          'The selected network has no tracked headers yet. Wait for synchronization or lower first tracked height.',
      }
    }
    return null
  }

  const emptyState = getEmptyState()

  return (
    <TooltipProvider delayDuration={300}>
      <div className="relative isolate flex h-screen w-screen flex-col bg-background text-foreground">
        <AppHeader
          networks={networks}
          selectedNetworkId={selectedNetworkId}
          onNetworkChange={handleNetworkChange}
          themePreference={themePreference}
          onCycleTheme={cycleTheme}
          blockCount={processedBlocks.length}
          reachableNodes={reachableNodes}
          totalNodes={totalNodes}
          connectionStatus={connectionStatus}
        />

        {data && selectedNetwork !== null && (
          <NodeSection key={selectedNetwork.id} network={selectedNetwork} nodes={data.nodes} />
        )}

        <BlockGraph
          nodes={nodes}
          edges={edges}
          themePreference={themePreference}
          connectionStatus={connectionStatus}
          staleError={dataError && data ? dataError : null}
          emptyState={emptyState}
          showFoldToggle={foldMeta.potentialFoldedSegmentCount > 0}
          globalCollapsed={globalCollapsed}
          onToggleGlobalCollapsed={() => setGlobalCollapsed(c => !c)}
        />

        {selectedBlock && !foldMeta.hiddenBlockIds.has(selectedBlockId!) && (
          <BlockDetailPanel block={selectedBlock} onClose={() => setSelectedBlockId(null)} />
        )}
      </div>
    </TooltipProvider>
  )
}

export default App
