import { ReactFlow } from '@xyflow/react'
import '@xyflow/react/dist/style.css'
import { useCallback, useEffect, useMemo, useState } from 'react'
import { Separator } from '@/components/ui/separator'
import { TooltipProvider } from '@/components/ui/tooltip'
import { ActiveNodeInfoCard } from './ActiveNodeCard'
import { BlockDetailPanel } from './BlockDetailPanel'
import { BlockNode, type BlockNodeType } from './BlockNode'
import { ConnectionStatus } from './ConnectionStatus'
import { useNetworkData, useNetworks } from './hooks'
import { Legend } from './Legend'
import { NetworkSelector } from './NetworkSelector'
import { ThemeToggle } from './ThemeToggle'
import { buildReactFlowGraph, preprocessData } from './tree'

const nodeTypes = { block: BlockNode }

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

function CenteredState({ title, message }: { title: string; message: string }) {
  return (
    <div className="flex h-full flex-col items-center justify-center px-6 text-center">
      <h2 className="text-base font-semibold text-foreground">{title}</h2>
      <p className="mt-1 max-w-md text-sm text-muted-foreground">{message}</p>
    </div>
  )
}

function MetricsDivider() {
  return <Separator orientation="vertical" className="h-3!" />
}

function App() {
  const { networks, loading: networksLoading, error: networksError } = useNetworks()
  const [selectedNetworkId, setSelectedNetworkId] = useState<number | null>(getNetworkIdFromUrl)
  const [selectedBlockId, setSelectedBlockId] = useState<number | null>(null)

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

  const { nodes, edges } = useMemo(() => {
    if (processedBlocks.length === 0) {
      return { nodes: [] as BlockNodeType[], edges: [] }
    }

    return buildReactFlowGraph(processedBlocks, block => handleBlockClick(block.id), selectedBlockId)
  }, [processedBlocks, handleBlockClick, selectedBlockId])

  if (networksLoading) {
    return (
      <div className="flex h-screen items-center justify-center bg-background px-6 text-center">
        <p className="text-sm text-muted-foreground">Loading network configuration…</p>
      </div>
    )
  }

  if (networksError) {
    return (
      <div className="flex h-screen items-center justify-center bg-background px-6">
        <div className="max-w-lg rounded-lg border border-red-400/40 bg-red-500/5 p-5 text-sm text-red-600">
          Could not load network list: {networksError}
        </div>
      </div>
    )
  }

  if (networks.length === 0) {
    return (
      <div className="flex h-screen items-center justify-center bg-background px-6">
        <div className="max-w-lg rounded-lg border border-border bg-muted/20 p-5 text-sm text-muted-foreground">
          No networks configured.
        </div>
      </div>
    )
  }

  const hasNoBlocks = Boolean(data && processedBlocks.length === 0)
  const showConnectionWarning = connectionStatus === 'error' || connectionStatus === 'closed'
  const totalNodes = data?.nodes.length ?? 0
  const reachableNodes = data?.nodes.filter(node => node.reachable).length ?? 0

  return (
    <TooltipProvider delayDuration={300}>
      <div className="flex h-screen w-screen flex-col bg-background text-foreground">
        <header className="border-b border-border px-4 py-3 sm:px-6">
          <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
            <div className="min-w-0">
              <h1 className="text-lg font-semibold tracking-tight text-foreground">Reorg Playground</h1>
              <p className="text-[13px] text-muted-foreground">
                Watch how Bitcoin nodes see the chain — forks, tips, and reorgs as they happen.
              </p>
            </div>

            <div className="flex shrink-0 items-center gap-2">
              <NetworkSelector networks={networks} selectedId={selectedNetworkId} onChange={handleNetworkChange} />
              <ThemeToggle />
            </div>
          </div>

          <div className="mt-2.5 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
            <span>
              <span className="font-medium text-foreground">{processedBlocks.length.toLocaleString()}</span> blocks
            </span>
            <MetricsDivider />
            <span>
              <span className="font-medium text-foreground">{totalNodes}</span> nodes
            </span>
            <MetricsDivider />
            <span>
              <span className="font-medium text-foreground">{reachableNodes}</span>/{totalNodes} reachable
            </span>
            <MetricsDivider />
            <ConnectionStatus status={connectionStatus} />
          </div>
        </header>

        {data && <ActiveNodeInfoCard nodes={data.nodes} />}

        <main className="relative min-h-0 flex-1">
          {showConnectionWarning && (
            <div className="border-b border-amber-400/40 bg-amber-500/10 px-4 py-2 text-xs text-amber-700 sm:px-6">
              Live updates are currently degraded ({connectionStatus}). Displayed data may be stale.
            </div>
          )}

          {dataError && data && (
            <div className="border-b border-red-400/40 bg-red-500/10 px-4 py-2 text-xs text-red-600 sm:px-6">
              Could not refresh latest data: {dataError}
            </div>
          )}

          <div className="h-full">
            {selectedNetworkId === null ? (
              <CenteredState title="Select a network" message="Choose a configured network to load blockchain data." />
            ) : dataLoading && !data ? (
              <CenteredState
                title="Loading chain data"
                message="Fetching latest tips and headers from configured nodes."
              />
            ) : dataError && !data ? (
              <CenteredState title="Failed to load chain data" message={dataError} />
            ) : hasNoBlocks ? (
              <CenteredState
                title="No blocks to render"
                message="The selected network has no tracked headers yet. Wait for synchronization or lower first tracked height."
              />
            ) : (
              <ReactFlow
                nodes={nodes}
                edges={edges}
                nodeTypes={nodeTypes}
                nodesDraggable={false}
                nodesConnectable={false}
                fitView
                fitViewOptions={{ padding: 0.25, duration: 200 }}
                minZoom={0.05}
                maxZoom={1.4}
                onlyRenderVisibleElements
                proOptions={{ hideAttribution: true }}
              >
                <Legend />
              </ReactFlow>
            )}
          </div>
        </main>

        {selectedBlock && <BlockDetailPanel block={selectedBlock} onClose={() => setSelectedBlockId(null)} />}
      </div>
    </TooltipProvider>
  )
}

export default App
