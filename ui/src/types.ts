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
  tips: TipInfo[]
  last_changed_timestamp: number
  version: string
  reachable: boolean
}

export type Network = {
  id: number
  name: string
  description: string
}

export type NetworksResponse = {
  networks: Network[]
}

export type DataResponse = {
  header_infos: HeaderInfo[]
  nodes: NodeInfo[]
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
