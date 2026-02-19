import type { Edge } from '@xyflow/react'
import { MarkerType } from '@xyflow/react'
import type { BlockNodeType } from './BlockNode'
import type { DataResponse, ProcessedBlock, TipStatusEntry } from './types'
import { MAX_PREV_ID, type TipStatus } from './types'

/**
 * Annotate each block with aggregated tip status info from all nodes,
 * and build a children map.
 */
export function preprocessData(data: DataResponse): ProcessedBlock[] {
  const blockMap = new Map<number, ProcessedBlock>()

  // Initialize all blocks
  for (const h of data.header_infos) {
    blockMap.set(h.id, {
      ...h,
      tipStatuses: [],
      children: [],
    })
  }

  // Aggregate tip statuses per block from all nodes
  const tipAgg = new Map<string, Map<TipStatus, string[]>>()
  for (const node of data.nodes) {
    for (const tip of node.tips) {
      // Find the block by hash
      const block = data.header_infos.find(h => h.hash === tip.hash)
      if (!block) continue
      const key = String(block.id)
      if (!tipAgg.has(key)) tipAgg.set(key, new Map())
      const statusMap = tipAgg.get(key)!
      if (!statusMap.has(tip.status)) statusMap.set(tip.status, [])
      statusMap.get(tip.status)!.push(node.name)
    }
  }

  for (const [blockIdStr, statusMap] of tipAgg) {
    const block = blockMap.get(Number(blockIdStr))
    if (!block) continue
    const entries: TipStatusEntry[] = []
    for (const [status, nodeNames] of statusMap) {
      entries.push({ status, nodeNames })
    }
    block.tipStatuses = entries
  }

  // Build children links
  for (const block of blockMap.values()) {
    if (block.prev_id !== MAX_PREV_ID) {
      const parent = blockMap.get(block.prev_id)
      if (parent) parent.children.push(block.id)
    }
  }

  return [...blockMap.values()]
}

const H_GAP = 250
const V_GAP = 100

/**
 * Convert processed blocks into React Flow nodes and edges.
 */
export function buildReactFlowGraph(
  blocks: ProcessedBlock[],
  onBlockClick: (block: ProcessedBlock) => void,
): { nodes: BlockNodeType[]; edges: Edge[] } {
  const blockMap = new Map<number, ProcessedBlock>()
  for (const b of blocks) blockMap.set(b.id, b)

  // Rebuild children map for this subset
  const childrenMap = new Map<number, number[]>()
  for (const b of blocks) {
    if (b.prev_id !== MAX_PREV_ID && blockMap.has(b.prev_id)) {
      const siblings = childrenMap.get(b.prev_id) || []
      siblings.push(b.id)
      childrenMap.set(b.prev_id, siblings)
    }
  }

  // Assign positions using recursive tree layout
  let leafIndex = 0
  const positions = new Map<number, { x: number; y: number }>()

  // Build a height-to-depth mapping for consistent horizontal spacing
  const heights = [...new Set(blocks.map(b => b.height))].sort((a, b) => a - b)
  const heightToDepth = new Map<number, number>()
  for (let i = 0; i < heights.length; i++) {
    heightToDepth.set(heights[i], i)
  }

  function assign(blockId: number): number {
    const block = blockMap.get(blockId)!
    const depth = heightToDepth.get(block.height) ?? 0
    const children = childrenMap.get(blockId) || []

    if (children.length === 0) {
      const idx = leafIndex++
      positions.set(blockId, { x: depth * H_GAP, y: idx * V_GAP })
      return idx
    }

    const childIdxs = children.map(c => assign(c))
    const mid = (childIdxs[0] + childIdxs[childIdxs.length - 1]) / 2
    positions.set(blockId, { x: depth * H_GAP, y: mid * V_GAP })
    return mid
  }

  // Find roots and assign positions
  const roots = blocks.filter(b => b.prev_id === MAX_PREV_ID || !blockMap.has(b.prev_id))
  for (const root of roots) {
    if (!positions.has(root.id)) assign(root.id)
  }

  // Build nodes
  const nodes: BlockNodeType[] = blocks.map(block => ({
    id: String(block.id),
    type: 'block' as const,
    position: positions.get(block.id) ?? { x: 0, y: 0 },
    data: {
      height: block.height,
      hash: block.hash,
      miner: block.miner,
      tipStatuses: block.tipStatuses,
      difficultyInt: block.difficulty_int,
      onBlockClick: () => onBlockClick(block),
    },
  }))

  // Build edges
  const edges: Edge[] = blocks
    .filter(b => b.prev_id !== MAX_PREV_ID && blockMap.has(b.prev_id))
    .map(b => ({
      id: `${b.prev_id}-${b.id}`,
      source: String(b.prev_id),
      target: String(b.id),
      type: 'smoothstep',
      markerEnd: { type: MarkerType.ArrowClosed },
    }))

  return { nodes, edges }
}
