import { ArrowDownUp, Info, Plug, Unplug } from "lucide-react";
import { useMemo, useRef, useState } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";
import { Separator } from "@/components/ui/separator";
import { Spinner } from "@/components/ui/spinner";
import {
	Tooltip,
	TooltipContent,
	TooltipTrigger,
} from "@/components/ui/tooltip";
import {
	type PendingConnect,
	usePeerConnectionManager,
} from "@/hooks/usePeerConnectionManager";
import type {
	NodeP2PAddress,
	PeerInfo,
} from "@/services/peerConnectionManagerService";
import type { P2PControl } from "./ActiveNodeCard";
import type { Network, NodeInfo } from "./types";

type PeerConnectionManagerProps = {
	network: Network;
	nodes: NodeInfo[];
	getP2PControl: (node: NodeInfo) => P2PControl;
	buttonClassName?: string;
};

type AddNodeFn = (
	nodeId: number,
	nodeName: string,
	targetNodeId: number,
	targetLabel: string,
	address: string,
) => Promise<boolean>;

type DisconnectFn = (
	nodeId: number,
	nodeName: string,
	peerLabel: string,
	address: string,
	peerId: number,
	addnodeRemoveAddresses: string[],
	counterpartyNodeId: number | null,
	localListenAddressCandidates: string[],
	matchedNodeId: number | null,
) => Promise<boolean>;

/** Strings to pass to Bitcoin Core `addnode … remove` (must match what was used for `add`). */
function addnodeRemovalCandidates(
	peer: PeerInfo,
	nodeP2PAddresses: NodeP2PAddress[],
): string[] {
	const catalog =
		peer.matched_node_id != null
			? nodeP2PAddresses.find((a) => a.node_id === peer.matched_node_id)
					?.address
			: undefined;
	const out = [...(catalog ? [catalog] : []), peer.addr];
	return [...new Set(out.filter(Boolean))];
}

function PeerRow({
	peer,
	disconnecting,
	disconnectDisabled,
	showDisconnectAction,
	onDisconnect,
}: {
	peer: PeerInfo;
	disconnecting: boolean;
	disconnectDisabled: boolean;
	showDisconnectAction: boolean;
	onDisconnect: () => void;
}) {
	const name = peer.matched_node_name;
	const direction = peer.inbound ? "inbound" : "outbound";

	return (
		<li
			className={[
				"flex items-center justify-between gap-2 rounded-md px-2 py-1",
				disconnecting ? "opacity-40" : "hover:bg-muted/40",
			].join(" ")}
		>
			<div className="flex items-center gap-2">
				{disconnecting && <Spinner className="size-3 text-muted-foreground" />}
				{name ? (
					<span className="text-sm font-medium text-foreground">{name}</span>
				) : (
					<span className="text-sm text-muted-foreground">External peer</span>
				)}
				<span className="text-xs text-muted-foreground">{direction}</span>
			</div>
			<div className="flex items-center gap-0.5">
				<Tooltip>
					<TooltipTrigger asChild>
						<span className="inline-flex size-6 items-center justify-center text-muted-foreground/60">
							<Info className="size-3.5" />
						</span>
					</TooltipTrigger>
					<TooltipContent side="top" className="text-xs">
						<span className="font-mono">{peer.addr}</span>
					</TooltipContent>
				</Tooltip>
				{showDisconnectAction && (
					<Tooltip>
						<TooltipTrigger asChild>
							<Button
								type="button"
								variant="ghost"
								size="icon-xs"
								className="text-destructive/60 hover:text-destructive"
								disabled={disconnecting || disconnectDisabled}
								onClick={onDisconnect}
							>
								<Unplug className="size-3.5" />
							</Button>
						</TooltipTrigger>
						<TooltipContent side="top" className="text-xs">
							Disconnect
						</TooltipContent>
					</Tooltip>
				)}
			</div>
		</li>
	);
}

function PendingConnectRow({ label }: { label: string }) {
	return (
		<li className="flex items-center gap-2 rounded-md px-2 py-1 opacity-60">
			<Spinner className="size-3 text-muted-foreground" />
			<span className="text-sm text-muted-foreground">{label}</span>
		</li>
	);
}

function getEmptyStateLabel(p2p: P2PControl): string {
	if (p2p.loading || p2p.active == null) {
		return "Checking P2P networking...";
	}

	if (!p2p.active) {
		return "P2P networking disabled";
	}

	return "No connected peers";
}

function P2PToggleButton({ p2p }: { p2p: P2PControl }) {
	if (!p2p.supported) return null;

	const unknown = p2p.active == null;
	let label = "Enable P2P";
	if (p2p.loading) label = "Updating...";
	else if (unknown) label = "Checking...";
	else if (p2p.active) label = "Disable P2P";
	const showSpinner = p2p.loading || unknown;

	return (
		<Tooltip>
			<TooltipTrigger asChild>
				<span>
					<Button
						type="button"
						variant={p2p.active ? "outline" : "default"}
						size="xs"
						onClick={p2p.onToggle}
						disabled={p2p.loading || unknown}
					>
						{showSpinner ? (
							<Spinner className="size-3" />
						) : (
							<ArrowDownUp className="size-3" />
						)}
						{label}
					</Button>
				</span>
			</TooltipTrigger>
			<TooltipContent side="top" className="max-w-56 text-xs">
				Turn on/off the P2P networking. Reconnection can take ~30 seconds. Easy
				way to create reorgs.
			</TooltipContent>
		</Tooltip>
	);
}

function ConnectToNodeSelect({
	node,
	availableTargets,
	disabled,
	onConnect,
}: {
	node: NodeInfo;
	availableTargets: NodeP2PAddress[];
	disabled: boolean;
	onConnect: AddNodeFn;
}) {
	const [submitting, setSubmitting] = useState(false);
	const [selectedTargetNodeId, setSelectedTargetNodeId] = useState<string>();

	const handleSelectNode = async (targetNodeId: string) => {
		const target = availableTargets.find(
			(t) => String(t.node_id) === targetNodeId,
		);
		if (!target) return;
		setSelectedTargetNodeId(targetNodeId);
		setSubmitting(true);
		try {
			await onConnect(
				node.id,
				node.name,
				target.node_id,
				target.name,
				target.address,
			);
		} finally {
			setSelectedTargetNodeId(undefined);
			setSubmitting(false);
		}
	};

	if (availableTargets.length === 0) return null;

	return (
		<Select
			value={selectedTargetNodeId}
			onValueChange={handleSelectNode}
			disabled={disabled || submitting}
		>
			<SelectTrigger
				size="sm"
				className="h-7 gap-1.5 px-2.5 text-xs"
				disabled={disabled || submitting}
			>
				{submitting ? (
					<Spinner className="size-3" />
				) : (
					<Plug className="size-3" />
				)}
				<SelectValue placeholder="Connect to..." />
			</SelectTrigger>
			<SelectContent>
				{availableTargets.map((target) => (
					<SelectItem key={target.node_id} value={String(target.node_id)}>
						{target.name}
					</SelectItem>
				))}
			</SelectContent>
		</Select>
	);
}

function ReachabilityBadge({ reachable }: { reachable: boolean }) {
	return (
		<Badge
			variant={reachable ? "secondary" : "destructive"}
			className={[
				"h-5 max-w-full rounded-full px-2 py-0.5 text-xs font-medium",
				reachable
					? "border-success/40 bg-success/10 text-success"
					: "border-destructive/40 bg-destructive/10 text-destructive",
			].join(" ")}
		>
			<span
				className={[
					"size-2 rounded-full",
					reachable ? "bg-success" : "bg-destructive",
				].join(" ")}
				aria-hidden="true"
			/>
			{reachable ? "Reachable" : "Unreachable"}
		</Badge>
	);
}

function getNodePairKey(nodeId: number, relatedNodeId: number): string {
	return [nodeId, relatedNodeId].sort((a, b) => a - b).join(":");
}

function isPeerDisconnecting(
	nodeId: number,
	peer: PeerInfo,
	disconnectingPeers: Set<string>,
	disconnectingConnectionPairs: Set<string>,
): boolean {
	if (disconnectingPeers.has(`${nodeId}:${peer.id}`)) {
		return true;
	}

	return (
		peer.matched_node_id != null &&
		disconnectingConnectionPairs.has(
			getNodePairKey(nodeId, peer.matched_node_id),
		)
	);
}

function NodePeerSection({
	node,
	peers,
	p2p,
	viewOnly,
	disconnectingPeers,
	disconnectingConnectionPairs,
	outgoingPendingConnectsForNode,
	incomingPendingConnectsForNode,
	availableTargets,
	nodeP2PAddresses,
	onAddNode,
	onDisconnect,
}: {
	node: NodeInfo;
	peers: PeerInfo[];
	p2p: P2PControl;
	viewOnly: boolean;
	disconnectingPeers: Set<string>;
	disconnectingConnectionPairs: Set<string>;
	outgoingPendingConnectsForNode: PendingConnect[];
	incomingPendingConnectsForNode: PendingConnect[];
	availableTargets: NodeP2PAddress[];
	nodeP2PAddresses: NodeP2PAddress[];
	onAddNode: AddNodeFn;
	onDisconnect: DisconnectFn;
}) {
	const totalCount =
		peers.length +
		outgoingPendingConnectsForNode.length +
		incomingPendingConnectsForNode.length;
	const connectionLabel =
		totalCount === 1 ? "1 connection" : `${totalCount} connections`;
	const disconnectingPeerCount = peers.filter((peer) =>
		isPeerDisconnecting(
			node.id,
			peer,
			disconnectingPeers,
			disconnectingConnectionPairs,
		),
	).length;
	const hasAnyLoading =
		disconnectingPeerCount > 0 ||
		outgoingPendingConnectsForNode.length > 0 ||
		incomingPendingConnectsForNode.length > 0;
	const canToggleP2P = !viewOnly && p2p.supported;
	const peerActionsEnabled =
		canToggleP2P && p2p.active === true && !p2p.loading;
	const waitingForReconnect = p2p.waitingForReconnect;

	return (
		<Card
			className={[
				"panel-glass gap-0 rounded-2xl py-0",
				!node.reachable && "border-destructive/40 bg-destructive/10",
			]
				.filter(Boolean)
				.join(" ")}
		>
			<CardHeader className="gap-0 px-3 pt-2.5 pb-2">
				<div className="flex flex-wrap items-center justify-between gap-2">
					<div className="flex items-center gap-2">
						<CardTitle className="text-sm leading-tight">{node.name}</CardTitle>
						<ReachabilityBadge reachable={node.reachable} />
						<span className="text-xs text-muted-foreground">
							{connectionLabel}
						</span>
					</div>
					{canToggleP2P && <P2PToggleButton p2p={p2p} />}
				</div>
			</CardHeader>

			<CardContent className="space-y-2 px-3 pt-0 pb-2.5">
				{(peers.length > 0 ||
					outgoingPendingConnectsForNode.length > 0 ||
					incomingPendingConnectsForNode.length > 0 ||
					waitingForReconnect) && (
					<ul className="space-y-1">
						{peers.map((peer) => {
							const isDisconnecting = isPeerDisconnecting(
								node.id,
								peer,
								disconnectingPeers,
								disconnectingConnectionPairs,
							);
							return (
								<PeerRow
									key={peer.id}
									peer={peer}
									disconnecting={isDisconnecting}
									disconnectDisabled={!peerActionsEnabled}
									showDisconnectAction={canToggleP2P}
									onDisconnect={() => {
										const label = peer.matched_node_name ?? peer.addr;
										void onDisconnect(
											node.id,
											node.name,
											label,
											peer.addr,
											peer.id,
											addnodeRemovalCandidates(peer, nodeP2PAddresses),
											peer.matched_node_id ?? null,
											nodeP2PAddresses
												.filter((a) => a.node_id === node.id)
												.map((a) => a.address),
											peer.matched_node_id ?? null,
										);
									}}
								/>
							);
						})}
						{outgoingPendingConnectsForNode.map((pc) => (
							<PendingConnectRow
								key={`pending-outgoing-${pc.id}`}
								label={`Connecting to ${pc.targetName}...`}
							/>
						))}
						{incomingPendingConnectsForNode.map((pc) => (
							<PendingConnectRow
								key={`pending-incoming-${pc.id}`}
								label={`Connecting from ${pc.nodeName}...`}
							/>
						))}
						{waitingForReconnect && (
							<PendingConnectRow label="Waiting for reconnection..." />
						)}
					</ul>
				)}

				{peers.length === 0 &&
					outgoingPendingConnectsForNode.length === 0 &&
					incomingPendingConnectsForNode.length === 0 &&
					!waitingForReconnect && (
						<p className="text-xs text-muted-foreground">
							{getEmptyStateLabel(p2p)}
						</p>
					)}

				{canToggleP2P && availableTargets.length > 0 && (
					<>
						<Separator />
						<ConnectToNodeSelect
							node={node}
							availableTargets={availableTargets}
							disabled={
								hasAnyLoading || waitingForReconnect || !peerActionsEnabled
							}
							onConnect={onAddNode}
						/>
					</>
				)}
			</CardContent>
		</Card>
	);
}

export function PeerConnectionManager({
	network,
	nodes,
	getP2PControl,
	buttonClassName,
}: PeerConnectionManagerProps) {
	const [dialogOpen, setDialogOpen] = useState(false);
	const dialogTitleRef = useRef<HTMLHeadingElement>(null);
	const {
		peerData,
		isLoading,
		isRefreshing,
		error,
		connectionStatus,
		pendingConnects,
		disconnectingPeers,
		disconnectingConnectionPairs,
		addNode: handleAddNode,
		disconnectNode: handleDisconnect,
	} = usePeerConnectionManager(dialogOpen ? network : null);

	if (nodes.length < 2) return null;

	const peersByNodeId = new Map(
		peerData?.nodes.map((n) => [n.node_id, n.peers]) ?? [],
	);
	const nodeP2PAddresses = peerData?.node_p2p_addresses ?? [];

	return (
		<>
			<Button
				variant="outline"
				size="xs"
				className={buttonClassName}
				onClick={() => setDialogOpen(true)}
			>
				<ArrowDownUp className="size-3.5" />
				Manage Connections
			</Button>

			<Dialog open={dialogOpen} onOpenChange={setDialogOpen}>
				<DialogContent
					className="max-h-[85vh] overflow-y-auto sm:max-w-2xl"
					onOpenAutoFocus={(event) => {
						event.preventDefault();
						dialogTitleRef.current?.focus();
					}}
				>
					<DialogHeader>
						<DialogTitle ref={dialogTitleRef} tabIndex={-1}>
							Node Connections
						</DialogTitle>
						<DialogDescription>
							Manage connections between your nodes on {network.name}.
						</DialogDescription>
						<p className="text-xs text-muted-foreground">
							Live status: {connectionStatus}
							{isRefreshing ? " • refreshing" : ""}
							{error ? ` • ${error}` : ""}
						</p>
					</DialogHeader>

					{isLoading && !peerData ? (
						<div className="flex flex-col items-center justify-center gap-2 py-8">
							<Spinner className="size-5 text-muted-foreground" />
							<p className="text-sm text-muted-foreground">
								Loading connection state...
							</p>
						</div>
					) : (
						<div className="space-y-3">
							{nodes.map((node) => {
								const nodePeers = peersByNodeId.get(node.id) ?? [];
								return (
									<NodePeerSectionWithTargets
										key={node.id}
										node={node}
										peers={nodePeers}
										p2p={getP2PControl(node)}
										viewOnly={network.view_only_mode}
										disconnectingPeers={disconnectingPeers}
										disconnectingConnectionPairs={disconnectingConnectionPairs}
										pendingConnects={pendingConnects}
										nodeP2PAddresses={nodeP2PAddresses}
										onAddNode={handleAddNode}
										onDisconnect={handleDisconnect}
									/>
								);
							})}
						</div>
					)}

					<DialogFooter showCloseButton />
				</DialogContent>
			</Dialog>
		</>
	);
}

/**
 * Wrapper that computes available connection targets for a node by filtering out
 * the node itself, peers it's already connected to, and pending connections.
 */
function NodePeerSectionWithTargets({
	node,
	peers,
	p2p,
	viewOnly,
	disconnectingPeers,
	disconnectingConnectionPairs,
	pendingConnects,
	nodeP2PAddresses,
	onAddNode,
	onDisconnect,
}: {
	node: NodeInfo;
	peers: PeerInfo[];
	p2p: P2PControl;
	viewOnly: boolean;
	disconnectingPeers: Set<string>;
	disconnectingConnectionPairs: Set<string>;
	pendingConnects: PendingConnect[];
	nodeP2PAddresses: NodeP2PAddress[];
	onAddNode: AddNodeFn;
	onDisconnect: DisconnectFn;
}) {
	const outgoingPendingConnectsForNode = useMemo(
		() => pendingConnects.filter((pc) => pc.nodeId === node.id),
		[pendingConnects, node.id],
	);
	const incomingPendingConnectsForNode = useMemo(
		() => pendingConnects.filter((pc) => pc.targetNodeId === node.id),
		[pendingConnects, node.id],
	);

	const connectedNodeIds = useMemo(() => {
		const ids = new Set<number>();
		for (const peer of peers) {
			if (peer.matched_node_id != null) {
				ids.add(peer.matched_node_id);
			}
		}
		return ids;
	}, [peers]);

	const pendingRelatedNodeIds = useMemo(() => {
		const ids = new Set<number>();
		for (const pendingConnect of outgoingPendingConnectsForNode) {
			ids.add(pendingConnect.targetNodeId);
		}
		for (const pendingConnect of incomingPendingConnectsForNode) {
			ids.add(pendingConnect.nodeId);
		}
		return ids;
	}, [incomingPendingConnectsForNode, outgoingPendingConnectsForNode]);

	const availableTargets = useMemo(
		() =>
			nodeP2PAddresses.filter(
				(a) =>
					a.node_id !== node.id &&
					!connectedNodeIds.has(a.node_id) &&
					!pendingRelatedNodeIds.has(a.node_id),
			),
		[nodeP2PAddresses, node.id, connectedNodeIds, pendingRelatedNodeIds],
	);

	return (
		<NodePeerSection
			node={node}
			peers={peers}
			p2p={p2p}
			viewOnly={viewOnly}
			disconnectingPeers={disconnectingPeers}
			disconnectingConnectionPairs={disconnectingConnectionPairs}
			outgoingPendingConnectsForNode={outgoingPendingConnectsForNode}
			incomingPendingConnectsForNode={incomingPendingConnectsForNode}
			availableTargets={availableTargets}
			nodeP2PAddresses={nodeP2PAddresses}
			onAddNode={onAddNode}
			onDisconnect={onDisconnect}
		/>
	);
}
