import { type Edge, MarkerType } from '@xyflow/react'
import type { BlockTreeNodeType } from './BlockTreeNode'
import type { FoldedBlockTreeNodeType } from './FoldedBlockTreeNode'
import type { MineTreeNodeType } from './MineTreeNode'
import {
  type DataResponse,
  MAX_PREV_ID,
  type NodeInfo,
  type ProcessedBlock,
  type TipStatus,
  type TipStatusEntry,
} from './types'

const H_GAP = 300
const V_GAP = 160
const BLOCK_NODE_HEIGHT = 144
const FOLDED_NODE_HEIGHT = 96

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

// ── Fold types ──────────────────────────────────────────────────────────

export type FoldSegment = {
  id: string
  startHeight: number
  endHeight: number
  hiddenCount: number
  blockIds: number[]
  predecessorBlockId: number | null
  successorBlockId: number | null
}

export type FoldMetadata = {
  potentialFoldedSegmentCount: number
  activeFoldedSegmentCount: number
  hiddenBlockIds: Set<number>
}

// ── Fold algorithm ──────────────────────────────────────────────────────

function computeFoldSegments(
  blocks: ProcessedBlock[],
  blockMap: Map<number, ProcessedBlock>,
  childrenMap: Map<number, number[]>,
): FoldSegment[] {
  if (blocks.length === 0) return []

  // Build parent map (child → parent)
  const parentMap = new Map<number, number>()
  for (const block of blocks) {
    if (block.prev_id !== MAX_PREV_ID && blockMap.has(block.prev_id)) {
      parentMap.set(block.id, block.prev_id)
    }
  }

  // Group by height
  const blocksByHeight = new Map<number, number[]>()
  for (const block of blocks) {
    if (!blocksByHeight.has(block.height)) blocksByHeight.set(block.height, [])
    blocksByHeight.get(block.height)!.push(block.id)
  }

  // Fork heights: any height with more than one block
  const forkHeights = new Set<number>()
  for (const [height, ids] of blocksByHeight) {
    if (ids.length > 1) forkHeights.add(height)
  }

  // Protected blocks: near forks or with tip statuses
  const protectedIds = new Set<number>()
  for (const block of blocks) {
    if (forkHeights.has(block.height) || forkHeights.has(block.height - 1) || forkHeights.has(block.height + 1)) {
      protectedIds.add(block.id)
    }
    if (block.tipStatuses.length > 0) {
      protectedIds.add(block.id)
    }
  }

  // Find linear chains by walking from chain starts.
  // A chain start is a block whose parent is missing from blockMap or
  // whose parent has multiple children (fork point).
  const visited = new Set<number>()
  const chains: number[][] = []

  for (const block of blocks) {
    if (visited.has(block.id)) continue

    const pid = parentMap.get(block.id)
    const parent = pid !== undefined ? blockMap.get(pid) : undefined
    const isChainStart =
      pid === undefined ||
      (childrenMap.get(pid)?.length ?? 0) > 1 ||
      (parent !== undefined && parent.height !== block.height - 1)

    if (!isChainStart) continue

    const chain: number[] = []
    let cur: number | undefined = block.id
    while (cur !== undefined && !visited.has(cur)) {
      const currentBlock = blockMap.get(cur)
      if (!currentBlock) break

      visited.add(cur)
      chain.push(cur)

      const childIds: number[] = childrenMap.get(cur) ?? []
      if (childIds.length !== 1) {
        cur = undefined
        continue
      }

      const nextId = childIds[0]
      const nextBlock = blockMap.get(nextId)
      if (!nextBlock || nextBlock.height !== currentBlock.height + 1) {
        cur = undefined
        continue
      }

      cur = nextId
    }

    if (chain.length > 0) chains.push(chain)
  }

  // Defensive: pick up any blocks not yet visited
  for (const block of blocks) {
    if (!visited.has(block.id)) {
      visited.add(block.id)
      chains.push([block.id])
    }
  }

  // Within each chain, find contiguous runs of unprotected blocks
  // with at least 3 blocks to fold.
  const segments: FoldSegment[] = []

  for (const chain of chains) {
    let runStart = -1

    for (let i = 0; i <= chain.length; i++) {
      const blockId = i < chain.length ? chain[i] : undefined
      const isEnd = blockId === undefined || protectedIds.has(blockId)

      if (isEnd) {
        if (runStart >= 0) {
          const runIds = chain.slice(runStart, i)
          if (runIds.length >= 3) {
            const first = blockMap.get(runIds[0])!
            const last = blockMap.get(runIds[runIds.length - 1])!
            segments.push({
              id: `${first.id}-${last.id}`,
              startHeight: first.height,
              endHeight: last.height,
              hiddenCount: runIds.length,
              blockIds: runIds,
              predecessorBlockId: runStart > 0 ? chain[runStart - 1] : null,
              successorBlockId: i < chain.length ? chain[i] : null,
            })
          }
          runStart = -1
        }
      } else {
        if (runStart < 0) runStart = i
      }
    }
  }

  return segments
}

// ── React Flow graph builder ────────────────────────────────────────────

export type FlowNodeType = BlockTreeNodeType | MineTreeNodeType | FoldedBlockTreeNodeType

export function buildReactFlowGraph(
  blocks: ProcessedBlock[],
  onBlockClick: (block: ProcessedBlock) => void,
  selectedBlockId: number | null = null,
  networkId: number | null = null,
  networkType: string | null = null,
  allNodes: NodeInfo[] = [],
  globalCollapsed: boolean = false,
): { nodes: FlowNodeType[]; edges: Edge[]; foldMeta: FoldMetadata } {
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

  // ── Slot assignment (uses full tree, unaffected by folding) ──────────

  let leafCursor = 0
  const visiting = new Set<number>()
  const slotById = new Map<number, number>()

  function assignSlot(blockId: number): number {
    const existingSlot = slotById.get(blockId)
    if (existingSlot !== undefined) return existingSlot

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

  for (const block of blocks) {
    if (slotById.has(block.id)) continue
    leafCursor += 1
    assignSlot(block.id)
  }

  // ── Fold computation ─────────────────────────────────────────────────

  const foldSegments = computeFoldSegments(blocks, blockMap, childrenMap)
  const collapsedSegments = globalCollapsed ? foldSegments : []

  const hiddenBlockIds = new Set<number>()
  for (const seg of collapsedSegments) {
    for (const id of seg.blockIds) hiddenBlockIds.add(id)
  }

  const visibleBlocks = blocks.filter(b => !hiddenBlockIds.has(b.id))

  // ── Compressed depth mapping ─────────────────────────────────────────
  // Build ordered column list from visible heights + fold placeholders.

  type Column =
    | { sortKey: number; kind: 'height'; height: number }
    | { sortKey: number; kind: 'fold'; segmentId: string }

  const visibleHeightSet = new Set(visibleBlocks.map(b => b.height))
  const columns: Column[] = [...visibleHeightSet]
    .sort((a, b) => a - b)
    .map(h => ({ sortKey: h, kind: 'height' as const, height: h }))

  if (collapsedSegments.length > 0) {
    for (const seg of collapsedSegments) {
      columns.push({
        sortKey: seg.startHeight,
        kind: 'fold' as const,
        segmentId: seg.id,
      })
    }
    columns.sort((a, b) => a.sortKey - b.sortKey)
  }

  const heightToDepth = new Map<number, number>()
  const foldToDepth = new Map<string, number>()

  for (let i = 0; i < columns.length; i++) {
    const col = columns[i]
    if (col.kind === 'height') {
      heightToDepth.set(col.height, i)
    } else {
      foldToDepth.set(col.segmentId, i)
    }
  }

  // ── Block nodes ──────────────────────────────────────────────────────

  const blockNodes: BlockTreeNodeType[] = visibleBlocks.map(block => {
    const depth = heightToDepth.get(block.height) ?? 0
    const slot = slotById.get(block.id) ?? 0

    return {
      id: String(block.id),
      type: 'block' as const,
      position: { x: depth * H_GAP, y: slot * V_GAP },
      width: 240,
      height: BLOCK_NODE_HEIGHT,
      selected: selectedBlockId === block.id,
      data: {
        height: block.height,
        hash: block.hash,
        miner: block.miner,
        tipStatuses: block.tipStatuses,
        onBlockClick: () => onBlockClick(block),
      },
    }
  })

  // ── Folded nodes (vertically centered relative to block nodes) ───────

  const foldedYOffset = (BLOCK_NODE_HEIGHT - FOLDED_NODE_HEIGHT) / 2

  const foldedNodes: FoldedBlockTreeNodeType[] = collapsedSegments.map(seg => {
    const depth = foldToDepth.get(seg.id) ?? 0
    const slot = slotById.get(seg.blockIds[0]) ?? 0

    return {
      id: `fold-${seg.id}`,
      type: 'folded' as const,
      selectable: false,
      position: { x: depth * H_GAP, y: slot * V_GAP + foldedYOffset },
      width: 192,
      height: FOLDED_NODE_HEIGHT,
      data: {
        startHeight: seg.startHeight,
        endHeight: seg.endHeight,
        hiddenCount: seg.hiddenCount,
      },
    }
  })

  // ── Edges ────────────────────────────────────────────────────────────

  const blockEdges: Edge[] = []

  // Regular edges between visible blocks
  for (const block of visibleBlocks) {
    if (block.prev_id === MAX_PREV_ID || !blockMap.has(block.prev_id)) continue
    // Skip if parent is hidden (edge handled by fold boundary edges)
    if (hiddenBlockIds.has(block.prev_id)) continue

    const highlightsSelected =
      selectedBlockId !== null && (selectedBlockId === block.id || selectedBlockId === block.prev_id)

    blockEdges.push({
      id: `${block.prev_id}-${block.id}`,
      source: String(block.prev_id),
      target: String(block.id),
      type: 'smoothstep',
      markerEnd: { type: MarkerType.ArrowClosed },
      style: {
        stroke: highlightsSelected ? 'var(--accent)' : 'var(--border)',
        strokeWidth: highlightsSelected ? 2 : 1.5,
      },
    })
  }

  // Fold boundary edges (predecessor → fold → successor)
  for (const seg of collapsedSegments) {
    const foldNodeId = `fold-${seg.id}`

    if (seg.predecessorBlockId !== null) {
      blockEdges.push({
        id: `${seg.predecessorBlockId}-${foldNodeId}`,
        source: String(seg.predecessorBlockId),
        target: foldNodeId,
        type: 'smoothstep',
        markerEnd: { type: MarkerType.ArrowClosed },
        style: {
          stroke: 'var(--border)',
          strokeWidth: 1.5,
          strokeDasharray: '6 4',
        },
      })
    }

    if (seg.successorBlockId !== null) {
      blockEdges.push({
        id: `${foldNodeId}-${seg.successorBlockId}`,
        source: foldNodeId,
        target: String(seg.successorBlockId),
        type: 'smoothstep',
        markerEnd: { type: MarkerType.ArrowClosed },
        style: {
          stroke: 'var(--border)',
          strokeWidth: 1.5,
          strokeDasharray: '6 4',
        },
      })
    }
  }

  // ── Mine nodes (only for visible real blocks) ────────────────────────

  const mineNodes: MineTreeNodeType[] = []
  const mineEdges: Edge[] = []

  if (networkType === 'Regtest' && networkId !== null) {
    for (const block of visibleBlocks) {
      const activeEntry = block.tipStatuses.find(tipStatus => tipStatus.status === 'active')
      if (!activeEntry) continue

      const activeNodeNames = new Set(activeEntry.nodeNames)
      const hasMineableNode = allNodes.some(
        node => node.implementation === 'Bitcoin Core' && activeNodeNames.has(node.name),
      )
      if (!hasMineableNode) continue

      const depth = heightToDepth.get(block.height) ?? 0
      const slot = slotById.get(block.id) ?? 0
      const mineNodeId = `mine-${block.id}`
      const highlightsSelected = selectedBlockId !== null && selectedBlockId === block.id

      mineNodes.push({
        id: mineNodeId,
        type: 'mine',
        position: { x: (depth + 1) * H_GAP, y: slot * V_GAP },
        width: 100,
        height: BLOCK_NODE_HEIGHT,
        selected: highlightsSelected,
        data: {
          block,
          networkId,
          nodes: allNodes,
        },
      })

      mineEdges.push({
        id: `${block.id}-${mineNodeId}`,
        source: String(block.id),
        target: mineNodeId,
        type: 'smoothstep',
        markerEnd: { type: MarkerType.ArrowClosed },
        style: {
          stroke: 'var(--accent)',
          strokeWidth: highlightsSelected ? 2 : 1.5,
          strokeDasharray: '5 4',
        },
      })
    }
  }

  return {
    nodes: [...blockNodes, ...foldedNodes, ...mineNodes],
    edges: [...blockEdges, ...mineEdges],
    foldMeta: {
      potentialFoldedSegmentCount: foldSegments.length,
      activeFoldedSegmentCount: collapsedSegments.length,
      hiddenBlockIds,
    },
  }
}
