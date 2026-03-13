import { ChevronDown, ChevronUp } from 'lucide-react'
import { AnimatePresence, motion } from 'motion/react'
import { useEffect, useMemo, useState } from 'react'
import { Card, CardContent } from '@/components/ui/card'
import { useNodeP2PConnection } from '@/hooks/useNodeP2PConnection'
import { ActiveNodeCard, activeTip } from './ActiveNodeCard'
import type { Network, NodeInfo } from './types'

const RELATIVE_TIME_REFRESH_MS = 10_000

export function NodeSection({ network, nodes }: { network: Network; nodes: NodeInfo[] }) {
  const [collapsed, setCollapsed] = useState(false)
  const [, setTick] = useState(0)

  const {
    toggleNodeP2PConnection,
    getNodeP2PConnectionActive,
    isEnabledByNodeId: p2pEnabledById,
    loadingByNodeId: p2pLoadingById,
  } = useNodeP2PConnection(network, nodes)

  useEffect(() => {
    const interval = window.setInterval(() => setTick(t => t + 1), RELATIVE_TIME_REFRESH_MS)
    return () => window.clearInterval(interval)
  }, [])

  const sortedNodes = useMemo(() => {
    return [...nodes].sort((a, b) => a.name.localeCompare(b.name))
  }, [nodes])

  const maxHeight = useMemo(() => {
    return Math.max(0, ...sortedNodes.map(n => activeTip(n)?.height ?? 0))
  }, [sortedNodes])

  return (
    <div className="relative z-10">
      <AnimatePresence initial={false}>
        {!collapsed && (
          <motion.div
            id="node-health-panel"
            key="node-health-panel"
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: 'auto', opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
            transition={{ duration: 0.25, ease: [0.4, 0, 0.2, 1] }}
            style={{ overflow: 'hidden' }}
          >
            <section className="px-3 pb-1.5 sm:px-4 lg:px-5" aria-label="Node health panel">
              {sortedNodes.length === 0 ? (
                <Card className="panel-glass gap-0 rounded-2xl border-dashed py-0">
                  <CardContent className="p-3">
                    <p className="text-sm text-muted-foreground">No node status data yet.</p>
                  </CardContent>
                </Card>
              ) : (
                <div className="flex max-h-184 flex-wrap gap-2 overflow-y-auto pb-1.5">
                  {sortedNodes.map(node => (
                    <ActiveNodeCard
                      key={node.id}
                      node={node}
                      maxHeight={maxHeight}
                      p2p={{
                        supported: p2pEnabledById[node.id] ?? false,
                        active: getNodeP2PConnectionActive(node.id) ?? null,
                        loading: p2pLoadingById[node.id] ?? false,
                        onToggle: () => void toggleNodeP2PConnection(node),
                      }}
                    />
                  ))}
                </div>
              )}
            </section>
          </motion.div>
        )}
      </AnimatePresence>

      <button
        type="button"
        onClick={() => setCollapsed(c => !c)}
        aria-controls="node-health-panel"
        aria-expanded={!collapsed}
        aria-label={collapsed ? 'Show node panel' : 'Hide node panel'}
        className="absolute -bottom-3.5 left-1/2 z-30 flex size-7 -translate-x-1/2 cursor-pointer items-center justify-center rounded-full border border-border/70 [background:var(--surface-panel-strong)] shadow-(--elevation-soft) backdrop-blur-md text-muted-foreground/70 transition-colors duration-150 hover:text-foreground"
      >
        {collapsed ? <ChevronDown className="size-4" /> : <ChevronUp className="size-4" />}
      </button>
    </div>
  )
}
