import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import useSWR from 'swr'
import { openCacheChangesStream } from '../services/cacheChangeEventService'
import { fetchNetworkSnapshot } from '../services/networkSnapshotService'
import { getNetworkSnapshotKey, type NetworkSnapshotKey } from '../services/swrKeys'
import type { ConnectionStatus, DataResponse } from '../types'
import { useNotification } from './useNotification'

const REFRESH_DEBOUNCE_MS = 150
const EVENT_CACHE_CHANGED = 'cache_changed'
const EVENT_RESYNC_REQUIRED = 'resync_required'

type CacheChangedEvent = {
  network_id?: number
}

function mapEventSourceReadyStateToConnectionStatus(readyState: number): ConnectionStatus {
  if (readyState === EventSource.CONNECTING) return 'connecting'
  if (readyState === EventSource.CLOSED) return 'closed'
  return 'error'
}

function parseCacheChangedNetworkId(event: Event): number | null {
  const messageEvent = event as MessageEvent<string>
  try {
    const parsed = JSON.parse(messageEvent.data) as CacheChangedEvent
    return typeof parsed.network_id === 'number' ? parsed.network_id : null
  } catch {
    return null
  }
}

function fetchNetworkSnapshotByKey([, networkId]: NetworkSnapshotKey) {
  return fetchNetworkSnapshot(networkId)
}

function getInitialErrorToastId(networkId: number | null) {
  return networkId === null ? undefined : `network-data-initial:${networkId}`
}

function getStaleErrorToastId(networkId: number | null) {
  return networkId === null ? undefined : `network-data-stale:${networkId}`
}

function getConnectionToastId(networkId: number | null) {
  return networkId === null ? undefined : `network-data-connection:${networkId}`
}

export function useNetworkData(networkId: number | null) {
  const { notifyError, dismissNotification } = useNotification()
  const [connectionStatus, setConnectionStatus] = useState<ConnectionStatus>(
    networkId === null ? 'closed' : 'connecting',
  )
  const refreshTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  const snapshotKey = useMemo(() => (networkId === null ? null : getNetworkSnapshotKey(networkId)), [networkId])

  const { data, error, isLoading, mutate } = useSWR<DataResponse, Error, NetworkSnapshotKey | null>(
    snapshotKey,
    fetchNetworkSnapshotByKey,
    {
      revalidateOnFocus: false,
      revalidateOnReconnect: false,
      keepPreviousData: false,
      onError: currentError => {
        const hasCurrentData = Boolean(data)

        notifyError({
          id: hasCurrentData ? getStaleErrorToastId(networkId) : getInitialErrorToastId(networkId),
          title: hasCurrentData ? 'Could not refresh latest data' : 'Could not load chain data',
          description: currentError.message,
        })
      },
      onSuccess: () => {
        dismissNotification(getInitialErrorToastId(networkId))
        dismissNotification(getStaleErrorToastId(networkId))
      },
    },
  )

  const clearScheduledRefresh = useCallback(() => {
    if (refreshTimerRef.current) {
      clearTimeout(refreshTimerRef.current)
      refreshTimerRef.current = null
    }
  }, [])

  const scheduleRefresh = useCallback(() => {
    clearScheduledRefresh()
    refreshTimerRef.current = setTimeout(() => {
      void mutate()
    }, REFRESH_DEBOUNCE_MS)
  }, [clearScheduledRefresh, mutate])

  useEffect(() => {
    clearScheduledRefresh()

    if (networkId === null) {
      dismissNotification(getConnectionToastId(networkId))
      setConnectionStatus('closed')
      return
    }

    setConnectionStatus('connecting')

    const eventSource = openCacheChangesStream(networkId)

    const handleCacheChanged = (event: Event) => {
      const changedNetworkId = parseCacheChangedNetworkId(event)
      if (changedNetworkId !== null && changedNetworkId !== networkId) {
        return
      }
      scheduleRefresh()
    }

    const handleResyncRequired = () => {
      scheduleRefresh()
    }

    eventSource.onopen = () => {
      dismissNotification(getConnectionToastId(networkId))
      setConnectionStatus('connected')
    }
    eventSource.onerror = () => {
      const nextStatus = mapEventSourceReadyStateToConnectionStatus(eventSource.readyState)

      setConnectionStatus(currentStatus => {
        if ((nextStatus === 'error' || nextStatus === 'closed') && currentStatus !== nextStatus) {
          notifyError({
            id: getConnectionToastId(networkId),
            title: 'Live updates are degraded',
            description:
              nextStatus === 'closed'
                ? 'Live updates are disconnected. Displayed data may be stale.'
                : 'Live updates encountered an error. Displayed data may be stale.',
          })
        }

        return nextStatus
      })
    }

    eventSource.addEventListener(EVENT_CACHE_CHANGED, handleCacheChanged)
    eventSource.addEventListener(EVENT_RESYNC_REQUIRED, handleResyncRequired)

    return () => {
      eventSource.removeEventListener(EVENT_CACHE_CHANGED, handleCacheChanged)
      eventSource.removeEventListener(EVENT_RESYNC_REQUIRED, handleResyncRequired)
      eventSource.close()
      dismissNotification(getConnectionToastId(networkId))
      clearScheduledRefresh()
    }
  }, [networkId, scheduleRefresh, clearScheduledRefresh, notifyError, dismissNotification])

  return {
    data: networkId === null ? null : (data ?? null),
    loading: networkId === null ? false : isLoading,
    error: networkId === null ? null : (error?.message ?? null),
    connectionStatus,
  }
}
