import { type Edge, MarkerType } from '@xyflow/react'
import type { BlockNodeType } from './BlockNode'
import { type DataResponse, MAX_PREV_ID, type NodeInfo, type ProcessedBlock, type TipStatus, type TipStatusEntry } from './types'

const H_GAP = 300
const V_GAP = 160

const TIP_STATUS_ORDER: Record<TipStatus, number> = {
  active: 0,
  'valid-fork': 1,
  'valid-headers': 2,
  'headers-only': 3,
  invalid: 4,
  unknown: 5,
}

function compareBlocks(a: ProcessedBlock, b: ProcessedBlock): number {
  if (a.height !== b.height) return a.height - b.height
  const hashCompare = a.hash.localeCompare(b.hash)
  if (hashCompare !== 0) return hashCompare
  return a.id - b.id
}

/**
 * Annotate each block with aggregated tip status info from all nodes,
 * and build a children map.
 */
export function preprocessData(data: DataResponse): ProcessedBlock[] {
  const blockMap = new Map<number, ProcessedBlock>()
  const hashToBlockId = new Map<string, number>()

  for (const header of data.header_infos) {
    blockMap.set(header.id, {
      ...header,
      tipStatuses: [],
      children: [],
    })
    hashToBlockId.set(header.hash, header.id)
  }

  const tipAgg = new Map<number, Map<TipStatus, Set<string>>>()
  for (const node of data.nodes) {
    for (const tip of node.tips) {
      const blockId = hashToBlockId.get(tip.hash)
      if (blockId === undefined) continue

      if (!tipAgg.has(blockId)) {
        tipAgg.set(blockId, new Map())
      }

      const statusMap = tipAgg.get(blockId)
      if (!statusMap) continue

      if (!statusMap.has(tip.status)) {
        statusMap.set(tip.status, new Set())
      }

      statusMap.get(tip.status)?.add(node.name)
    }
  }

  for (const [blockId, statusMap] of tipAgg) {
    const block = blockMap.get(blockId)
    if (!block) continue

    const entries: TipStatusEntry[] = [...statusMap.entries()]
      .sort((a, b) => TIP_STATUS_ORDER[a[0]] - TIP_STATUS_ORDER[b[0]])
      .map(([status, nodeNames]) => ({
        status,
        nodeNames: [...nodeNames].sort((a, b) => a.localeCompare(b)),
      }))

    block.tipStatuses = entries
  }

  for (const block of blockMap.values()) {
    if (block.prev_id === MAX_PREV_ID) continue
    const parent = blockMap.get(block.prev_id)
    if (parent) {
      parent.children.push(block.id)
    }
  }

  return [...blockMap.values()].sort(compareBlocks)
}

/**
 * Convert processed blocks into React Flow nodes and edges.
 */
export function buildReactFlowGraph(
  blocks: ProcessedBlock[],
  onBlockClick: (block: ProcessedBlock) => void,
  selectedBlockId: number | null = null,
  networkId: number | null = null,
  networkType: string | null = null,
  allNodes: NodeInfo[] = [],
): { nodes: BlockNodeType[]; edges: Edge[] } {
  const blockMap = new Map<number, ProcessedBlock>()
  for (const block of blocks) {
    blockMap.set(block.id, block)
  }

  const childrenMap = new Map<number, number[]>()
  for (const block of blocks) {
    if (block.prev_id === MAX_PREV_ID || !blockMap.has(block.prev_id)) continue

    if (!childrenMap.has(block.prev_id)) {
      childrenMap.set(block.prev_id, [])
    }

    childrenMap.get(block.prev_id)?.push(block.id)
  }

  for (const childIds of childrenMap.values()) {
    childIds.sort((idA, idB) => {
      const blockA = blockMap.get(idA)
      const blockB = blockMap.get(idB)
      if (!blockA || !blockB) return 0
      return compareBlocks(blockA, blockB)
    })
  }

  const heights = [...new Set(blocks.map(block => block.height))].sort((a, b) => a - b)
  const heightToDepth = new Map<number, number>()
  for (let index = 0; index < heights.length; index++) {
    const height = heights[index]
    if (height !== undefined) {
      heightToDepth.set(height, index)
    }
  }

  let leafCursor = 0
  const visiting = new Set<number>()
  const slotById = new Map<number, number>()

  function assignSlot(blockId: number): number {
    const existingSlot = slotById.get(blockId)
    if (existingSlot !== undefined) return existingSlot

    // Defensive fallback for malformed graphs.
    if (visiting.has(blockId)) {
      const fallbackSlot = leafCursor++
      slotById.set(blockId, fallbackSlot)
      return fallbackSlot
    }

    const block = blockMap.get(blockId)
    if (!block) {
      const fallbackSlot = leafCursor++
      slotById.set(blockId, fallbackSlot)
      return fallbackSlot
    }

    visiting.add(blockId)

    const children = (childrenMap.get(blockId) ?? []).filter(childId => blockMap.has(childId))
    if (children.length === 0) {
      const slot = leafCursor++
      slotById.set(blockId, slot)
      visiting.delete(blockId)
      return slot
    }

    const childSlots = children.map(assignSlot)
    const minChildSlot = Math.min(...childSlots)
    const maxChildSlot = Math.max(...childSlots)
    const slot = (minChildSlot + maxChildSlot) / 2

    slotById.set(blockId, slot)
    visiting.delete(blockId)
    return slot
  }

  const roots = blocks
    .filter(block => block.prev_id === MAX_PREV_ID || !blockMap.has(block.prev_id))
    .sort(compareBlocks)

  roots.forEach((root, index) => {
    if (index > 0) {
      leafCursor += 1
    }
    assignSlot(root.id)
  })

  // Handle disconnected leftovers if any (defensive).
  for (const block of blocks) {
    if (slotById.has(block.id)) continue
    leafCursor += 1
    assignSlot(block.id)
  }

  const nodes: BlockNodeType[] = blocks.map(block => {
    const depth = heightToDepth.get(block.height) ?? 0
    const slot = slotById.get(block.id) ?? 0

    return {
      id: String(block.id),
      type: 'block' as const,
      position: { x: depth * H_GAP, y: slot * V_GAP },
      selected: selectedBlockId === block.id,
      data: {
        height: block.height,
        hash: block.hash,
        miner: block.miner,
        tipStatuses: block.tipStatuses,
        onBlockClick: () => onBlockClick(block),
        networkId,
        networkType,
        nodes: allNodes,
        block,
      },
    }
  })

  const edges: Edge[] = blocks
    .filter(block => block.prev_id !== MAX_PREV_ID && blockMap.has(block.prev_id))
    .map(block => {
      const highlightsSelected =
        selectedBlockId !== null && (selectedBlockId === block.id || selectedBlockId === block.prev_id)

      return {
        id: `${block.prev_id}-${block.id}`,
        source: String(block.prev_id),
        target: String(block.id),
        type: 'smoothstep',
        markerEnd: { type: MarkerType.ArrowClosed },
        style: {
          stroke: highlightsSelected ? 'var(--accent)' : 'var(--border)',
          strokeWidth: highlightsSelected ? 2 : 1.5,
        },
      }
    })

  return { nodes, edges }
}
