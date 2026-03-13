import { useCallback, useMemo } from 'react'
import { toast } from 'sonner'

type NotificationPayload = {
  title: string
  description?: string
  id?: string | number
  duration?: number
}

function getToastOptions({ description, duration, id }: NotificationPayload) {
  return {
    description,
    duration,
    id,
  }
}

export function useNotification() {
  const notifySuccess = useCallback((payload: NotificationPayload) => {
    toast.success(payload.title, getToastOptions(payload))
  }, [])

  const notifyError = useCallback((payload: NotificationPayload) => {
    toast.error(payload.title, getToastOptions(payload))
  }, [])

  const dismissNotification = useCallback((id?: string | number) => {
    toast.dismiss(id)
  }, [])

  return useMemo(
    () => ({ notifySuccess, notifyError, dismissNotification }),
    [dismissNotification, notifyError, notifySuccess],
  )
}
