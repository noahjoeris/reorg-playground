/** Opens the SSE stream that emits peer-change events from the backend. */
export function openPeerChangesStream(networkId?: number): EventSource {
  const suffix = networkId === undefined ? '' : `?network_id=${encodeURIComponent(networkId)}`
  return new EventSource(`/api/peer-changes${suffix}`)
}
