import { ReactFlow } from '@xyflow/react'
import '@xyflow/react/dist/style.css'
import { useCallback, useEffect, useMemo, useState } from 'react'
import { BlockDetailPanel } from './BlockDetailPanel'
import { BlockNode, type BlockNodeType } from './BlockNode'
import { ConnectionStatus } from './ConnectionStatus'
import { useInfo, useNetworkData, useNetworks } from './hooks'
import { Legend } from './Legend'
import { NetworkSelector } from './NetworkSelector'
import { NodeInfoPanel } from './NodeInfoPanel'
import { buildReactFlowGraph, preprocessData } from './tree'
import type { ProcessedBlock } from './types'

const nodeTypes = { block: BlockNode }

function getNetworkIdFromUrl(): number | null {
  const params = new URLSearchParams(window.location.search)
  const v = params.get('network')
  return v ? Number(v) : null
}

function setNetworkIdInUrl(id: number) {
  const url = new URL(window.location.href)
  url.searchParams.set('network', String(id))
  window.history.replaceState({}, '', url.toString())
}

function App() {
  const { networks, loading: networksLoading } = useNetworks()
  const [selectedNetworkId, setSelectedNetworkId] = useState<number | null>(getNetworkIdFromUrl)
  const [selectedBlock, setSelectedBlock] = useState<ProcessedBlock | null>(null)
  const { footer } = useInfo()

  // Set initial network from URL or first network
  useEffect(() => {
    if (selectedNetworkId !== null) return
    if (networks.length > 0) {
      setSelectedNetworkId(networks[0].id)
    }
  }, [networks, selectedNetworkId])

  const handleNetworkChange = useCallback((id: number) => {
    setSelectedNetworkId(id)
    setNetworkIdInUrl(id)
    setSelectedBlock(null)
  }, [])

  const { data, loading: dataLoading, connectionStatus } = useNetworkData(selectedNetworkId)

  const handleBlockClick = useCallback((block: ProcessedBlock) => {
    setSelectedBlock(block)
  }, [])

  const { nodes, edges } = useMemo(() => {
    if (!data) return { nodes: [] as BlockNodeType[], edges: [] }

    const processed = preprocessData(data)
    return buildReactFlowGraph(processed, handleBlockClick)
  }, [data, handleBlockClick])

  if (networksLoading) {
    return (
      <div className="flex h-screen items-center justify-center bg-background text-muted-foreground">Loading...</div>
    )
  }

  return (
    <div className="flex h-screen w-screen flex-col bg-background">
      {/* Header */}
      <div className="flex items-center justify-between border-b border-border px-4 py-2">
        <div className="flex items-center gap-4">
          <h1 className="text-lg font-bold text-foreground">Reorg Playground</h1>
          <NetworkSelector networks={networks} selectedId={selectedNetworkId} onChange={handleNetworkChange} />
        </div>
        <ConnectionStatus status={connectionStatus} />
      </div>

      {/* Node info cards */}
      {data && <NodeInfoPanel nodes={data.nodes} />}

      {/* React Flow tree */}
      <div className="relative flex-1">
        {dataLoading && !data ? (
          <div className="flex h-full items-center justify-center text-muted-foreground">Loading block data...</div>
        ) : (
          <ReactFlow
            nodes={nodes}
            edges={edges}
            nodeTypes={nodeTypes}
            nodesDraggable={false}
            nodesConnectable={false}
            fitView
            fitViewOptions={{ padding: 0.2 }}
            minZoom={0.05}
            proOptions={{ hideAttribution: true }}
          >
            <Legend />
          </ReactFlow>
        )}
      </div>

      {/* Block detail overlay */}
      {selectedBlock && <BlockDetailPanel block={selectedBlock} onClose={() => setSelectedBlock(null)} />}

      {/* Footer */}
      {footer && (
        <div className="border-t border-border px-4 py-1 text-center text-xs text-muted-foreground">{footer}</div>
      )}
    </div>
  )
}

export default App
