import { ReactFlow } from '@xyflow/react'
import '@xyflow/react/dist/style.css'
import { useCallback, useEffect, useMemo, useState } from 'react'
import { Button } from '@/components/ui/button'
import { Separator } from '@/components/ui/separator'
import { TooltipProvider } from '@/components/ui/tooltip'
import { ActiveNodeInfoCard } from './ActiveNodeCard'
import { BlockDetailPanel } from './BlockDetailPanel'
import { BlockTreeNode } from './BlockTreeNode'
import { ConnectionStatus } from './ConnectionStatus'
import { useNetworkData, useNetworks } from './hooks'
import { MineTreeNode } from './MineTreeNode'
import { NetworkSelector } from './NetworkSelector'
import { ThemeToggle } from './ThemeToggle'
import { buildReactFlowGraph, type FlowNodeType, preprocessData } from './tree'

const nodeTypes = { block: BlockTreeNode, mine: MineTreeNode }
const panelGlassClass =
  '[background:var(--surface-panel)] border border-border/70 shadow-[var(--elevation-soft)] backdrop-blur-[10px]'
const panelGlassStrongClass =
  '[background:var(--surface-panel-strong)] border border-accent/25 shadow-[var(--elevation-lift)] backdrop-blur-[12px]'
const metricPillClass = 'rounded-full border border-border/80 bg-card/70 px-2.5 py-[3px] text-[11px] tracking-[0.02em]'

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
      <div className={`${panelGlassStrongClass} max-w-lg rounded-2xl px-6 py-7`}>
        <p className="text-[11px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">Network State</p>
        <h2 className="mt-2 text-lg font-semibold text-foreground">{title}</h2>
        <p className="mt-2 text-sm leading-relaxed text-muted-foreground">{message}</p>
      </div>
    </div>
  )
}

function MetricsDivider() {
  return <Separator orientation="vertical" className="h-3.5 bg-border/70" />
}

function App() {
  const { networks, loading: networksLoading, error: networksError } = useNetworks()
  const [selectedNetworkId, setSelectedNetworkId] = useState<number | null>(getNetworkIdFromUrl)
  const [selectedBlockId, setSelectedBlockId] = useState<number | null>(null)
  const [isNodePanelCollapsed, setIsNodePanelCollapsed] = useState(false)

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

  const selectedNetwork = useMemo(() => {
    return networks.find(n => n.id === selectedNetworkId) ?? null
  }, [networks, selectedNetworkId])

  const { nodes, edges } = useMemo(() => {
    if (processedBlocks.length === 0) {
      return { nodes: [] as FlowNodeType[], edges: [] }
    }

    return buildReactFlowGraph(
      processedBlocks,
      block => handleBlockClick(block.id),
      selectedBlockId,
      selectedNetworkId,
      selectedNetwork?.network_type ?? null,
      data?.nodes ?? [],
    )
  }, [processedBlocks, handleBlockClick, selectedBlockId, selectedNetworkId, selectedNetwork, data])

  if (networksLoading) {
    return (
      <div className="relative isolate flex h-screen items-center justify-center bg-background px-6 text-center">
        <div className={`${panelGlassStrongClass} rounded-2xl px-6 py-5`}>
          <p className="text-sm text-muted-foreground">Loading network configuration...</p>
        </div>
      </div>
    )
  }

  if (networksError) {
    return (
      <div className="relative isolate flex h-screen items-center justify-center bg-background px-6">
        <div
          className={`${panelGlassStrongClass} max-w-lg rounded-2xl border-destructive/35 bg-destructive/10 p-5 text-sm text-destructive`}
        >
          Could not load network list: {networksError}
        </div>
      </div>
    )
  }

  if (networks.length === 0) {
    return (
      <div className="relative isolate flex h-screen items-center justify-center bg-background px-6">
        <div className={`${panelGlassStrongClass} max-w-lg rounded-2xl p-5 text-sm text-muted-foreground`}>
          No networks configured.
        </div>
      </div>
    )
  }

  const hasNoBlocks = Boolean(data && processedBlocks.length === 0)
  const showConnectionWarning = connectionStatus === 'error' || connectionStatus === 'closed'
  const totalNodes = data?.nodes.length ?? 0
  const reachableNodes = data?.nodes.filter(node => node.reachable).length ?? 0
  const showNodePanelToggle = Boolean(data)

  return (
    <TooltipProvider delayDuration={300}>
      <div className="relative isolate flex h-screen w-screen flex-col bg-background text-foreground">
        <header className="px-2.5 pt-2 pb-1.5 sm:px-3.5 sm:pt-2.5 lg:px-5 lg:pt-3">
          <div className={`${panelGlassStrongClass} rounded-2xl px-3 py-2 sm:px-4 sm:py-2.5`}>
            <div className="flex flex-col gap-1.5 sm:flex-row sm:items-start sm:justify-between">
              <div className="min-w-0">
                <h1 className="text-lg font-semibold tracking-tight text-foreground sm:text-xl">Reorg Playground</h1>
                <p className="mt-0.5 hidden max-w-2xl text-xs leading-relaxed text-muted-foreground sm:block">
                  Watch how nodes perceive forks, tips, and reorg events in real time.
                </p>
              </div>

              <div className="flex shrink-0 items-center gap-2">
                <NetworkSelector networks={networks} selectedId={selectedNetworkId} onChange={handleNetworkChange} />
                <ThemeToggle />
              </div>
            </div>

            <div className="mt-1.5 flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-muted-foreground">
              <span className={metricPillClass}>
                <span>{processedBlocks.length.toLocaleString()}</span> blocks
              </span>
              <MetricsDivider />
              <span className={metricPillClass}>
                {reachableNodes}/{totalNodes} nodes reachable
              </span>
              <MetricsDivider />
              <ConnectionStatus status={connectionStatus} />
              {showNodePanelToggle && (
                <>
                  <MetricsDivider />
                  <Button
                    type="button"
                    variant="outline"
                    size="xs"
                    className="rounded-full bg-background/65"
                    onClick={() => setIsNodePanelCollapsed(current => !current)}
                    aria-controls="node-health-panel"
                    aria-expanded={!isNodePanelCollapsed}
                    aria-label={isNodePanelCollapsed ? 'Show node panel' : 'Hide node panel'}
                  >
                    {isNodePanelCollapsed ? 'Show Nodes' : 'Hide Nodes'}
                  </Button>
                </>
              )}
            </div>
          </div>
        </header>

        {data && !isNodePanelCollapsed && (
          <div id="node-health-panel">
            <ActiveNodeInfoCard nodes={data.nodes} />
          </div>
        )}

        <main className="relative min-h-0 flex-1 px-2 pb-2 sm:px-3 sm:pb-2">
          <div className={`${panelGlassClass} relative h-full overflow-hidden rounded-2xl`}>
            {showConnectionWarning && (
              <div className="border-b border-warning/40 bg-warning/12 px-4 py-2 text-xs text-warning sm:px-6">
                Live updates are currently degraded ({connectionStatus}). Displayed data may be stale.
              </div>
            )}

            {dataError && data && (
              <div className="border-b border-destructive/40 bg-destructive/10 px-4 py-2 text-xs text-destructive sm:px-6">
                Could not refresh latest data: {dataError}
              </div>
            )}

            <div className="h-full">
              {selectedNetworkId === null ? (
                <CenteredState
                  title="Select a network"
                  message="Choose a configured network to load blockchain data."
                />
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
                  className="bg-transparent"
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
                  {/* <Legend /> */}
                </ReactFlow>
              )}
            </div>
          </div>
        </main>

        {selectedBlock && <BlockDetailPanel block={selectedBlock} onClose={() => setSelectedBlockId(null)} />}
      </div>
    </TooltipProvider>
  )
}

export default App
