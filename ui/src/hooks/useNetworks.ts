import useSWR from 'swr'
import { fetchNetworks } from '../services/networksService'
import { SWR_KEY_NETWORKS } from '../services/swrKeys'
import type { Network } from '../types'
import { useNotification } from './useNotification'

const NETWORKS_LOAD_ERROR_TOAST_ID = 'networks-load-error'

export function useNetworks() {
  const { notifyError, dismissNotification } = useNotification()
  const { data, error, isLoading } = useSWR<Network[], Error>(SWR_KEY_NETWORKS, () => fetchNetworks(), {
    revalidateOnFocus: false,
    revalidateOnReconnect: false,
    onError: currentError => {
      notifyError({
        id: NETWORKS_LOAD_ERROR_TOAST_ID,
        title: 'Could not load network list',
        description: currentError.message,
      })
    },
    onSuccess: () => {
      dismissNotification(NETWORKS_LOAD_ERROR_TOAST_ID)
    },
  })

  return {
    networks: data ?? [],
    loading: isLoading,
    error: error?.message ?? null,
  }
}
