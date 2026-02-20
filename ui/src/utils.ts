export function shortHash(hash: string, prefixLength = 10, suffixLength = 8): string {
  if (!hash) return ''
  if (hash.length <= prefixLength + suffixLength + 3) return hash
  return `${hash.slice(0, prefixLength)}...${hash.slice(-suffixLength)}`
}

export function formatMinerLabel(miner: string): string {
  const trimmed = miner.trim()
  if (!trimmed) return 'Unknown Miner'
  if (trimmed.toLowerCase() === 'unknown') return 'Unknown Miner'
  return trimmed
}
