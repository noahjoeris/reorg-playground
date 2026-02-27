/** Opens the SSE stream that emits `cache_changed` events from the backend. */
export function createCacheChangeEventSource(): EventSource {
  return new EventSource('api/changes')
}
