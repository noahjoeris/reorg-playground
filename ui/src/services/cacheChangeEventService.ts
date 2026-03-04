/** Opens the SSE stream that emits cache-change events from the backend. */
export function openCacheChangesStream(networkId?: number): EventSource {
  const suffix = networkId === undefined ? '' : `?network_id=${encodeURIComponent(networkId)}`
  return new EventSource(`/api/cache-changes${suffix}`)
}
