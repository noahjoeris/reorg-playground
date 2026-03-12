import { Controls, MiniMap, type Node, type OnInit, Panel, type ReactFlowInstance, ReactFlow } from '@xyflow/react'
import '@xyflow/react/dist/style.css'
import { useCallback, useMemo } from 'react'
import { Button } from '@/components/ui/button'
import type { ThemePreference } from '@/hooks/useTheme'
import { BlockTreeNode } from './BlockTreeNode'
import { FoldedBlockTreeNode } from './FoldedBlockTreeNode'
import { MineTreeNode } from './MineTreeNode'
import type { FlowNodeType } from './tree'
import { type ConnectionStatus, TIP_STATUS_COLORS, type TipStatusEntry } from './types'

const nodeTypes = { block: BlockTreeNode, mine: MineTreeNode, folded: FoldedBlockTreeNode }

function CenteredState({ title, message }: { title: string; message: string }) {
  return (
    <div className="flex h-full flex-col items-center justify-center px-6 text-center">
      <div className="panel-glass-strong max-w-lg rounded-2xl px-6 py-7">
        <p className="text-xs font-semibold uppercase tracking-widest text-muted-foreground">Network State</p>
        <h2 className="mt-2 text-lg font-semibold text-foreground">{title}</h2>
        <p className="mt-2 text-sm leading-relaxed text-muted-foreground">{message}</p>
      </div>
    </div>
  )
}

export function BlockGraph({
  nodes,
  edges,
  themePreference,
  connectionStatus,
  staleError,
  emptyState,
  showFoldToggle,
  globalCollapsed,
  onToggleGlobalCollapsed,
}: {
  nodes: FlowNodeType[]
  edges: { id: string; source: string; target: string }[]
  themePreference: ThemePreference
  connectionStatus: ConnectionStatus
  staleError: string | null
  emptyState: { title: string; message: string } | null
  showFoldToggle: boolean
  globalCollapsed: boolean
  onToggleGlobalCollapsed: () => void
}) {
  const showConnectionWarning = connectionStatus === 'error' || connectionStatus === 'closed'

  const fitViewOptions = useMemo(() => ({ padding: 0.25 }), [])

  const onInit: OnInit = useCallback((instance: ReactFlowInstance) => {
    instance.fitView(fitViewOptions)
  }, [fitViewOptions])

  const minimapNodeColor = useCallback((node: Node) => {
    if (node.type === 'block') {
      const statuses = (node.data as { tipStatuses?: TipStatusEntry[] }).tipStatuses
      if (statuses && statuses.length > 0) {
        return TIP_STATUS_COLORS[statuses[0].status]
      }
    }
    if (node.type === 'mine') return 'var(--accent)'
    if (node.type === 'folded') return 'var(--muted-foreground)'
    return 'var(--foreground)'
  }, [])

  return (
    <main className="relative min-h-128 flex-1 px-2 pb-2 sm:px-3 sm:pb-2">
      <div className="panel-glass relative h-full overflow-hidden rounded-2xl">
        {showConnectionWarning && (
          <div className="border-b border-warning/40 bg-warning/12 px-4 py-2 text-xs text-warning sm:px-6">
            Live updates are currently degraded ({connectionStatus}). Displayed data may be stale.
          </div>
        )}

        {staleError && (
          <div className="border-b border-destructive/40 bg-destructive/10 px-4 py-2 text-xs text-destructive sm:px-6">
            Could not refresh latest data: {staleError}
          </div>
        )}

        <div className="h-full">
          {emptyState ? (
            <CenteredState title={emptyState.title} message={emptyState.message} />
          ) : (
            <ReactFlow
              className="bg-transparent"
              colorMode={themePreference}
              nodes={nodes}
              edges={edges}
              nodeTypes={nodeTypes}
              nodesDraggable={false}
              nodesConnectable={false}
              onInit={onInit}
              minZoom={0.1}
              maxZoom={1.4}
              onlyRenderVisibleElements
              proOptions={{ hideAttribution: true }}
            >
              <MiniMap
                pannable
                nodeColor={minimapNodeColor}
                nodeStrokeColor="var(--accent)"
                bgColor="var(--muted)"
                inversePan
                maskColor="color-mix(in srgb, var(--muted) 50%, transparent)"
                className="hidden rounded-2xl border border-border md:block"
              />
              <Controls showInteractive={false} showZoom={false} />
              {showFoldToggle && (
                <Panel position="top-right" className="m-2">
                  <Button
                    type="button"
                    size="xs"
                    variant="outline"
                    className="rounded-full border-border/80 bg-card/90 px-3 font-semibold backdrop-blur"
                    onClick={onToggleGlobalCollapsed}
                    aria-label={globalCollapsed ? 'Expand all folded blocks' : 'Collapse uninteresting blocks'}
                    title={globalCollapsed ? 'Expand all' : 'Collapse all'}
                  >
                    {globalCollapsed ? 'Expand all' : 'Collapse all'}
                  </Button>
                </Panel>
              )}
            </ReactFlow>
          )}
        </div>
      </div>
    </main>
  )
}
