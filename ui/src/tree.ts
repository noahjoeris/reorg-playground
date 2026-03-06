import { type Edge, MarkerType } from '@xyflow/react'
import type { BlockTreeNodeType } from './BlockTreeNode'
import type { FoldedBlockTreeNodeType } from './FoldedBlockTreeNode'
import type { MineTreeNodeType } from './MineTreeNode'
import {
  type DataResponse,
  MAX_PREV_ID,
  type Network,
  type NodeInfo,
  type ProcessedBlock,
  type TipStatus,
  type TipStatusEntry,
} from './types'
import { isRegtestOrSignet } from './utils'

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

type SlotBounds = { min: number; max: number }
type ColumnDescriptor =
  | { sortKey: number; kind: 'height'; height: number }
  | { sortKey: number; kind: 'fold'; segmentId: string }
type DepthLookup = {
  heightToDepth: Map<number, number>
  foldToDepth: Map<string, number>
}
type TraversalFrame = { blockId: number; stage: 'enter' } | { blockId: number; stage: 'exit'; childIds: number[] }
type MineGraphElements = {
  mineNodes: MineTreeNodeType[]
  mineEdges: Edge[]
}

function buildBlockMap(blocks: ProcessedBlock[]): Map<number, ProcessedBlock> {
  const blockMap = new Map<number, ProcessedBlock>()
  for (const block of blocks) {
    blockMap.set(block.id, block)
  }
  return blockMap
}

function buildChildrenMap(
  blocks: ProcessedBlock[],
  blockMap: ReadonlyMap<number, ProcessedBlock>,
): Map<number, number[]> {
  const childrenByParentId = new Map<number, number[]>()

  for (const block of blocks) {
    if (block.prev_id === MAX_PREV_ID || !blockMap.has(block.prev_id)) continue
    const existingChildren = childrenByParentId.get(block.prev_id)
    if (existingChildren) {
      existingChildren.push(block.id)
    } else {
      childrenByParentId.set(block.prev_id, [block.id])
    }
  }

  for (const childIds of childrenByParentId.values()) {
    childIds.sort((leftId, rightId) => {
      const leftBlock = blockMap.get(leftId)
      const rightBlock = blockMap.get(rightId)
      if (!leftBlock || !rightBlock) return 0
      return compareBlocks(leftBlock, rightBlock)
    })
  }

  return childrenByParentId
}

function getRootBlocks(blocks: ProcessedBlock[], blockMap: ReadonlyMap<number, ProcessedBlock>): ProcessedBlock[] {
  return blocks.filter(block => block.prev_id === MAX_PREV_ID || !blockMap.has(block.prev_id)).sort(compareBlocks)
}

/**
 * Assigns deterministic vertical slots for each block in the full tree.
 * Leaves receive monotonically increasing integer slots; internal nodes are
 * centered between their children's min and max slots.
 */
function createSlotLayout(
  blocks: ProcessedBlock[],
  blockMap: ReadonlyMap<number, ProcessedBlock>,
  childrenMap: ReadonlyMap<number, number[]>,
): Map<number, number> {
  let nextLeafSlot = 0
  const slotByBlockId = new Map<number, number>()
  const visiting = new Set<number>()

  // Use an explicit stack so very long chains do not overflow the browser call stack.
  function assignSlot(startBlockId: number): number {
    const stack: TraversalFrame[] = [{ blockId: startBlockId, stage: 'enter' }]

    while (stack.length > 0) {
      const frame = stack.pop()
      if (!frame) break

      if (frame.stage === 'enter') {
        const existingSlot = slotByBlockId.get(frame.blockId)
        if (existingSlot !== undefined) continue

        if (visiting.has(frame.blockId)) {
          slotByBlockId.set(frame.blockId, nextLeafSlot++)
          continue
        }

        const block = blockMap.get(frame.blockId)
        if (!block) {
          slotByBlockId.set(frame.blockId, nextLeafSlot++)
          continue
        }

        const childIds = (childrenMap.get(frame.blockId) ?? []).filter(childId => blockMap.has(childId))
        if (childIds.length === 0) {
          slotByBlockId.set(frame.blockId, nextLeafSlot++)
          continue
        }

        visiting.add(frame.blockId)
        stack.push({ blockId: frame.blockId, stage: 'exit', childIds })

        for (let index = childIds.length - 1; index >= 0; index -= 1) {
          stack.push({ blockId: childIds[index], stage: 'enter' })
        }
        continue
      }

      visiting.delete(frame.blockId)

      const childSlots = frame.childIds
        .map(childId => slotByBlockId.get(childId))
        .filter((slot): slot is number => slot !== undefined)

      if (childSlots.length === 0) {
        slotByBlockId.set(frame.blockId, nextLeafSlot++)
        continue
      }

      const minChildSlot = Math.min(...childSlots)
      const maxChildSlot = Math.max(...childSlots)
      slotByBlockId.set(frame.blockId, (minChildSlot + maxChildSlot) / 2)
    }

    return slotByBlockId.get(startBlockId) ?? 0
  }

  const rootBlocks = getRootBlocks(blocks, blockMap)
  rootBlocks.forEach((rootBlock, index) => {
    if (index > 0) nextLeafSlot += 1
    assignSlot(rootBlock.id)
  })

  for (const block of blocks) {
    if (slotByBlockId.has(block.id)) continue
    nextLeafSlot += 1
    assignSlot(block.id)
  }

  return slotByBlockId
}

/**
 * Builds a resolver for the slot where a newly mined child block should appear.
 * When a tip has no children, mining extends on the same slot.
 * When a tip already has children, mining creates a fork sibling and is placed
 * below the deepest existing child subtree to avoid overlap.
 */
function createNextMineSlotResolver(
  slotById: ReadonlyMap<number, number>,
  childrenMap: ReadonlyMap<number, number[]>,
  blockMap: ReadonlyMap<number, ProcessedBlock>,
) {
  const subtreeBoundsByBlockId = new Map<number, SlotBounds>()
  const resolvingSubtree = new Set<number>()

  function getSubtreeBounds(startBlockId: number): SlotBounds {
    const cachedBounds = subtreeBoundsByBlockId.get(startBlockId)
    if (cachedBounds) return cachedBounds

    const stack: TraversalFrame[] = [{ blockId: startBlockId, stage: 'enter' }]

    while (stack.length > 0) {
      const frame = stack.pop()
      if (!frame) break

      if (frame.stage === 'enter') {
        const cached = subtreeBoundsByBlockId.get(frame.blockId)
        if (cached) continue

        const ownSlot = slotById.get(frame.blockId) ?? 0
        if (resolvingSubtree.has(frame.blockId)) {
          subtreeBoundsByBlockId.set(frame.blockId, { min: ownSlot, max: ownSlot })
          continue
        }

        const childIds = (childrenMap.get(frame.blockId) ?? []).filter(childId => blockMap.has(childId))
        if (childIds.length === 0) {
          subtreeBoundsByBlockId.set(frame.blockId, { min: ownSlot, max: ownSlot })
          continue
        }

        resolvingSubtree.add(frame.blockId)
        stack.push({ blockId: frame.blockId, stage: 'exit', childIds })

        for (let index = childIds.length - 1; index >= 0; index -= 1) {
          stack.push({ blockId: childIds[index], stage: 'enter' })
        }
        continue
      }

      const ownSlot = slotById.get(frame.blockId) ?? 0
      let minSlot = ownSlot
      let maxSlot = ownSlot

      for (const childId of frame.childIds) {
        const childBounds = subtreeBoundsByBlockId.get(childId)
        if (!childBounds) continue
        if (childBounds.min < minSlot) minSlot = childBounds.min
        if (childBounds.max > maxSlot) maxSlot = childBounds.max
      }

      resolvingSubtree.delete(frame.blockId)
      subtreeBoundsByBlockId.set(frame.blockId, { min: minSlot, max: maxSlot })
    }

    const bounds = subtreeBoundsByBlockId.get(startBlockId)
    if (bounds) return bounds

    const ownSlot = slotById.get(startBlockId) ?? 0
    return { min: ownSlot, max: ownSlot }
  }

  return (blockId: number): number => {
    const parentSlot = slotById.get(blockId) ?? 0
    const existingChildIds = (childrenMap.get(blockId) ?? []).filter(childId => blockMap.has(childId))
    if (existingChildIds.length === 0) return parentSlot

    const maxChildSubtreeSlot = Math.max(...existingChildIds.map(childId => getSubtreeBounds(childId).max))
    return Number.isFinite(maxChildSubtreeSlot) ? maxChildSubtreeSlot + 1 : parentSlot
  }
}

function createHiddenBlockIdSet(segments: FoldSegment[]): Set<number> {
  const hiddenBlockIds = new Set<number>()
  for (const segment of segments) {
    for (const blockId of segment.blockIds) hiddenBlockIds.add(blockId)
  }
  return hiddenBlockIds
}

/**
 * Maps heights and folded segments to compressed horizontal column indices.
 */
function createDepthLookup(visibleBlocks: ProcessedBlock[], collapsedSegments: FoldSegment[]): DepthLookup {
  const visibleHeights = [...new Set(visibleBlocks.map(block => block.height))].sort((a, b) => a - b)
  const columns: ColumnDescriptor[] = visibleHeights.map(height => ({ sortKey: height, kind: 'height', height }))

  if (collapsedSegments.length > 0) {
    for (const segment of collapsedSegments) {
      columns.push({
        sortKey: segment.startHeight,
        kind: 'fold',
        segmentId: segment.id,
      })
    }
    columns.sort((left, right) => left.sortKey - right.sortKey)
  }

  const heightToDepth = new Map<number, number>()
  const foldToDepth = new Map<string, number>()

  for (const [depth, column] of columns.entries()) {
    if (column.kind === 'height') {
      heightToDepth.set(column.height, depth)
    } else {
      foldToDepth.set(column.segmentId, depth)
    }
  }

  return { heightToDepth, foldToDepth }
}

function buildBlockNodes(
  visibleBlocks: ProcessedBlock[],
  heightToDepth: ReadonlyMap<number, number>,
  slotByBlockId: ReadonlyMap<number, number>,
  selectedBlockId: number | null,
  onBlockClick: (block: ProcessedBlock) => void,
): BlockTreeNodeType[] {
  return visibleBlocks.map(block => {
    const depth = heightToDepth.get(block.height) ?? 0
    const slot = slotByBlockId.get(block.id) ?? 0

    return {
      id: String(block.id),
      type: 'block',
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
}

function buildFoldedNodes(
  collapsedSegments: FoldSegment[],
  foldToDepth: ReadonlyMap<string, number>,
  slotByBlockId: ReadonlyMap<number, number>,
): FoldedBlockTreeNodeType[] {
  const foldedYOffset = (BLOCK_NODE_HEIGHT - FOLDED_NODE_HEIGHT) / 2

  return collapsedSegments.map(segment => {
    const depth = foldToDepth.get(segment.id) ?? 0
    const slot = slotByBlockId.get(segment.blockIds[0]) ?? 0

    return {
      id: `fold-${segment.id}`,
      type: 'folded',
      selectable: false,
      position: { x: depth * H_GAP, y: slot * V_GAP + foldedYOffset },
      width: 192,
      height: FOLDED_NODE_HEIGHT,
      data: {
        startHeight: segment.startHeight,
        endHeight: segment.endHeight,
        hiddenCount: segment.hiddenCount,
      },
    }
  })
}

function buildVisibleBlockEdges(
  visibleBlocks: ProcessedBlock[],
  blockMap: ReadonlyMap<number, ProcessedBlock>,
  hiddenBlockIds: ReadonlySet<number>,
  selectedBlockId: number | null,
): Edge[] {
  const edges: Edge[] = []

  for (const block of visibleBlocks) {
    if (block.prev_id === MAX_PREV_ID || !blockMap.has(block.prev_id)) continue
    if (hiddenBlockIds.has(block.prev_id)) continue

    const highlightsSelected =
      selectedBlockId !== null && (selectedBlockId === block.id || selectedBlockId === block.prev_id)

    edges.push({
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

  return edges
}

function buildFoldBoundaryEdges(collapsedSegments: FoldSegment[]): Edge[] {
  const edges: Edge[] = []

  for (const segment of collapsedSegments) {
    const foldNodeId = `fold-${segment.id}`

    if (segment.predecessorBlockId !== null) {
      edges.push({
        id: `${segment.predecessorBlockId}-${foldNodeId}`,
        source: String(segment.predecessorBlockId),
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

    if (segment.successorBlockId !== null) {
      edges.push({
        id: `${foldNodeId}-${segment.successorBlockId}`,
        source: foldNodeId,
        target: String(segment.successorBlockId),
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

  return edges
}

function hasMineableActiveTip(block: ProcessedBlock, allNodes: NodeInfo[]): boolean {
  const activeTip = block.tipStatuses.find(tipStatus => tipStatus.status === 'active')
  if (!activeTip) return false

  const activeTipNodeNames = new Set(activeTip.nodeNames)
  return allNodes.some(node => node.implementation === 'Bitcoin Core' && activeTipNodeNames.has(node.name))
}

function buildMineGraphElements(
  visibleBlocks: ProcessedBlock[],
  network: Network | null,
  allNodes: NodeInfo[],
  selectedBlockId: number | null,
  heightToDepth: ReadonlyMap<number, number>,
  resolveNextMineSlot: (blockId: number) => number,
): MineGraphElements {
  const mineNodes: MineTreeNodeType[] = []
  const mineEdges: Edge[] = []

  if (!network || network.disable_node_controls || !isRegtestOrSignet(network)) {
    return { mineNodes, mineEdges }
  }

  for (const block of visibleBlocks) {
    if (!hasMineableActiveTip(block, allNodes)) continue

    const depth = heightToDepth.get(block.height) ?? 0
    const mineSlot = resolveNextMineSlot(block.id)
    const mineNodeId = `mine-${block.id}`
    const highlightsSelected = selectedBlockId !== null && selectedBlockId === block.id

    mineNodes.push({
      id: mineNodeId,
      type: 'mine',
      position: { x: (depth + 1) * H_GAP, y: mineSlot * V_GAP },
      width: 100,
      height: BLOCK_NODE_HEIGHT,
      selected: highlightsSelected,
      data: {
        block,
        network,
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

  return { mineNodes, mineEdges }
}

export function buildReactFlowGraph(
  blocks: ProcessedBlock[],
  onBlockClick: (block: ProcessedBlock) => void,
  selectedBlockId: number | null = null,
  network: Network | null = null,
  allNodes: NodeInfo[] = [],
  globalCollapsed: boolean = false,
): { nodes: FlowNodeType[]; edges: Edge[]; foldMeta: FoldMetadata } {
  const blockMap = buildBlockMap(blocks)
  const childrenMap = buildChildrenMap(blocks, blockMap)
  const slotByBlockId = createSlotLayout(blocks, blockMap, childrenMap)
  const resolveNextMineSlot = createNextMineSlotResolver(slotByBlockId, childrenMap, blockMap)
  const foldSegments = computeFoldSegments(blocks, blockMap, childrenMap)
  const collapsedSegments = globalCollapsed ? foldSegments : []
  const hiddenBlockIds = createHiddenBlockIdSet(collapsedSegments)
  const visibleBlocks = blocks.filter(block => !hiddenBlockIds.has(block.id))
  const { heightToDepth, foldToDepth } = createDepthLookup(visibleBlocks, collapsedSegments)
  const blockNodes = buildBlockNodes(visibleBlocks, heightToDepth, slotByBlockId, selectedBlockId, onBlockClick)
  const foldedNodes = buildFoldedNodes(collapsedSegments, foldToDepth, slotByBlockId)
  const blockEdges = [
    ...buildVisibleBlockEdges(visibleBlocks, blockMap, hiddenBlockIds, selectedBlockId),
    ...buildFoldBoundaryEdges(collapsedSegments),
  ]
  const { mineNodes, mineEdges } = buildMineGraphElements(
    visibleBlocks,
    network,
    allNodes,
    selectedBlockId,
    heightToDepth,
    resolveNextMineSlot,
  )

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
