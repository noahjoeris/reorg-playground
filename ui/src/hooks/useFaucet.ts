import { useCallback } from "react";
import { mutate } from "swr";
import useSWRMutation from "swr/mutation";
import { sendFaucetTransaction } from "../services/faucetService";
import { getNetworkSnapshotKey } from "../services/swrKeys";
import type { FaucetResponse, Network } from "../types";
import { useNotification } from "./useNotification";

type FaucetMutationArgs = {
	networkId: number;
	nodeId: number;
	address: string;
	amountBtc: string;
};

const FAUCET_MUTATION_KEY = "faucet";

function successDescription(result: FaucetResponse, nodeName: string) {
	const refillSummary =
		(result.mined_blocks ?? 0) > 0
			? `Refilled the faucet with ${result.mined_blocks} mined block${result.mined_blocks === 1 ? "" : "s"} first.`
			: "No refill mining was needed.";
	const txidSummary = result.txid ? ` Txid: ${result.txid}` : "";
	return `Broadcast an unconfirmed transaction from ${nodeName}. ${refillSummary}${txidSummary}`;
}

export function useFaucet(network: Network) {
	const { notifyError, notifySuccess } = useNotification();
	const { trigger, isMutating } = useSWRMutation<
		FaucetResponse,
		Error,
		string,
		FaucetMutationArgs
	>(FAUCET_MUTATION_KEY, async (_key, { arg }) => {
		const result = await sendFaucetTransaction(arg.networkId, {
			node_id: arg.nodeId,
			address: arg.address,
			amount_btc: arg.amountBtc,
		});
		if (!result.success) {
			throw new Error(result.error ?? "Unknown error");
		}
		return result;
	});

	const send = useCallback(
		async (
			nodeId: number,
			nodeName: string,
			address: string,
			amountBtc: string,
		) => {
			try {
				const result = await trigger({
					networkId: network.id,
					nodeId,
					address,
					amountBtc,
				});
				void mutate(getNetworkSnapshotKey(network.id));
				notifySuccess({
					title: "Faucet transaction broadcast",
					description: successDescription(result, nodeName),
				});
				return result;
			} catch (err) {
				notifyError({
					title: "Could not send faucet transaction",
					description: err instanceof Error ? err.message : "Network error",
				});
				throw err;
			}
		},
		[network.id, notifyError, notifySuccess, trigger],
	);

	return { send, loading: isMutating };
}
