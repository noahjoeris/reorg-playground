import { useCallback, useRef, useState } from 'react'
import { mutate } from 'swr'
import { mineBlock } from '../services/miningService'
import { setNodeP2PConnectionActive } from '../services/nodeP2PConnectionService'
import { getNetworkSnapshotKey } from '../services/swrKeys'
import type { Network } from '../types'
import { useNotification } from './useNotification'

export type WorkflowStep = 'disconnecting' | 'mining-private' | 'mining-main' | 'reconnecting'

export type ReorgStep = 'idle' | WorkflowStep | 'done' | 'error'

export type ReorgState = {
  step: ReorgStep
  failedStep: WorkflowStep | null
  error: string | null
}

type TriggerReorgParams = {
  reorgNodeId: number
  reorgNodeName: string
  otherNodeId: number
  otherNodeName: string
  depth: number
}

const IDLE_STATE: ReorgState = { step: 'idle', failedStep: null, error: null }

export function useTriggerReorg(network: Network) {
  const [state, setState] = useState<ReorgState>(IDLE_STATE)
  const { notifyError, notifySuccess } = useNotification()
  const runningRef = useRef(false)

  const triggerReorg = useCallback(
    async (params: TriggerReorgParams) => {
      if (runningRef.current) return
      runningRef.current = true

      const { reorgNodeId, reorgNodeName, otherNodeId, otherNodeName, depth } = params
      const setStep = (step: WorkflowStep) => setState({ step, failedStep: null, error: null })

      try {
        setStep('disconnecting')
        const disconnectResult = await setNodeP2PConnectionActive(network.id, reorgNodeId, false)
        if (!disconnectResult.success) {
          throw new Error(disconnectResult.error ?? `Failed to disconnect ${reorgNodeName}`)
        }

        setStep('mining-private')
        const minePrivateResult = await mineBlock(network.id, reorgNodeId, depth)
        if (!minePrivateResult.success) {
          throw new Error(
            minePrivateResult.error ??
              `Failed to mine ${depth} blocks on ${reorgNodeName}. Node is still disconnected.`,
          )
        }

        setStep('mining-main')
        const mineMainResult = await mineBlock(network.id, otherNodeId, depth + 1)
        if (!mineMainResult.success) {
          throw new Error(
            mineMainResult.error ??
              `Failed to mine ${depth + 1} blocks on ${otherNodeName}. ${reorgNodeName} is still disconnected.`,
          )
        }

        setStep('reconnecting')
        const reconnectResult = await setNodeP2PConnectionActive(network.id, reorgNodeId, true)
        if (!reconnectResult.success) {
          throw new Error(reconnectResult.error ?? `Failed to reconnect ${reorgNodeName}`)
        }

        setState({ step: 'done', failedStep: null, error: null })
        void mutate(getNetworkSnapshotKey(network.id))

        notifySuccess({
          title: 'Reorg complete',
          description: 'It might take a few seconds for node reconnection.',
        })
      } catch (err) {
        const message = err instanceof Error ? err.message : 'Unknown error'
        setState(prev => ({
          step: 'error' as const,
          failedStep: isWorkflowStep(prev.step) ? prev.step : 'disconnecting',
          error: message,
        }))
        notifyError({ title: 'Reorg failed', description: message })
      } finally {
        runningRef.current = false
      }
    },
    [network.id, notifyError, notifySuccess],
  )

  const reset = useCallback(() => setState(IDLE_STATE), [])

  return { triggerReorg, state, reset }
}

function isWorkflowStep(step: ReorgStep): step is WorkflowStep {
  return step === 'disconnecting' || step === 'mining-private' || step === 'mining-main' || step === 'reconnecting'
}
