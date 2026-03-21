import type { NetworkMetrics } from './Metrics'

// Sentinel value for blocks with no parent (Rust's usize::MAX = 18446744073709551615).
// JS loses precision and rounds to 18446744073709552000, but JSON.parse produces
// the same rounded float, so equality checks still work.
export const MAX_PREV_ID = 18446744073709552000

export type TipStatus = 'active' | 'invalid' | 'valid-fork' | 'valid-headers' | 'headers-only' | 'unknown'

export const TIP_STATUS_COLORS: Record<TipStatus, string> = {
  active: 'var(--color-tip-active)',
  invalid: 'var(--color-tip-invalid)',
  'valid-fork': 'var(--color-tip-valid-fork)',
  'valid-headers': 'var(--color-tip-valid-headers)',
  'headers-only': 'var(--color-tip-headers-only)',
  unknown: 'var(--color-tip-unknown)',
}

export const TIP_STATUS_LABELS: Record<TipStatus, string> = {
  active: 'Active',
  invalid: 'Invalid',
  'valid-fork': 'Valid Fork',
  'valid-headers': 'Valid Headers',
  'headers-only': 'Headers Only',
  unknown: 'Unknown',
}

export const TIP_STATUS_DESCRIPTIONS: Record<TipStatus, string> = {
  active: 'This is the tip of the active main chain, which is certainly valid.',
  invalid: 'The branch contains at least one invalid block.',
  'valid-fork': 'This branch is not part of the active chain, but is fully validated.',
  'headers-only': 'Not all blocks for this branch are available, but the headers are valid.',
  'valid-headers': 'All blocks are available for this branch, but they were never fully validated.',
  unknown: 'Status is unknown.',
}

export type HeaderInfo = {
  id: number
  prev_id: number
  height: number
  hash: string
  prev_blockhash: string
  merkle_root: string
  time: number
  version: number
  nonce: number
  bits: number
  difficulty_int: number
  miner: string
}

export type TipInfo = {
  hash: string
  status: TipStatus
  height: number
}

export type NodeInfo = {
  id: number
  name: string
  description: string
  implementation: string
  supports_controls: boolean
  supports_mining: boolean
  supports_stale_tips: boolean
  tips: TipInfo[]
  last_changed_timestamp: number
  version: string
  reachable: boolean
}

export type NetworkType = 'Mainnet' | 'Testnet' | 'Signet' | 'Regtest'

export type Network = {
  id: number
  name: string
  description: string
  network_type: NetworkType
  view_only_mode: boolean
}

export type NetworksResponse = {
  networks: Network[]
}

export type DataResponse = {
  header_infos: HeaderInfo[]
  nodes: NodeInfo[]
  metrics: NetworkMetrics
}

export type DataChangedEvent = {
  network_id: number
}

// Internal processed types

export type TipStatusEntry = {
  status: TipStatus
  nodeNames: string[]
}

export type ProcessedBlock = HeaderInfo & {
  tipStatuses: TipStatusEntry[]
  children: number[]
}

export type ConnectionStatus = 'connecting' | 'connected' | 'error' | 'closed'

export type RpcActionResponse = {
  success: boolean
  error?: string
}

/** @deprecated Use RpcActionResponse instead. */
export type MineBlockResponse = RpcActionResponse
/** @deprecated Use RpcActionResponse instead. */
export type SetNodeP2PConnectionResponse = RpcActionResponse
/** @deprecated Use RpcActionResponse instead. */
export type PeerActionResponse = RpcActionResponse
