import { useMemo } from "react";
import { Badge } from "@/components/ui/badge";
import {
	Card,
	CardContent,
	CardDescription,
	CardHeader,
	CardTitle,
} from "@/components/ui/card";
import {
	Tooltip,
	TooltipContent,
	TooltipTrigger,
} from "@/components/ui/tooltip";
import {
	type NodeInfo,
	TIP_STATUS_COLORS,
	TIP_STATUS_DESCRIPTIONS,
	TIP_STATUS_LABELS,
	type TipStatus,
} from "./types";

const compactBadgeClass =
	"h-5 rounded-full bg-background/70 px-2 py-0.5 text-xs font-normal text-muted-foreground";

export function activeTip(node: NodeInfo) {
	return node.tips.find((tip) => tip.status === "active");
}

function tipStatusSummary(node: NodeInfo): Array<[TipStatus, number]> {
	const counts = new Map<TipStatus, number>();

	for (const tip of node.tips) {
		counts.set(tip.status, (counts.get(tip.status) ?? 0) + 1);
	}

	return [...counts.entries()].sort((a, b) => b[1] - a[1]);
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

function NodeMetric({
	label,
	value,
}: {
	label: string;
	value: string | number;
}) {
	return (
		<Badge variant="outline" className={compactBadgeClass}>
			<span>{label}</span>
			<span>{value}</span>
		</Badge>
	);
}

export type P2PControl = {
	supported: boolean;
	active: boolean | null;
	loading: boolean;
	waitingForReconnect: boolean;
	onToggle: () => void;
};

export function ActiveNodeCard({
	node,
	maxHeight,
}: {
	node: NodeInfo;
	maxHeight: number;
}) {
	const activeHeight = activeTip(node)?.height ?? 0;
	const lag = Math.max(0, maxHeight - activeHeight);
	const statusSummary = useMemo(() => tipStatusSummary(node), [node]);

	return (
		<Card
			className={[
				"panel-glass relative w-72 max-w-full shrink-0 gap-0 rounded-2xl py-0",
				"transition duration-200 ease-out",
				"hover:border-accent/35 hover:shadow-(--elevation-lift)",
				!node.reachable && "border-destructive/40 bg-destructive/10",
			]
				.filter(Boolean)
				.join(" ")}
			aria-label={`Node ${node.name}`}
		>
			<CardHeader className="gap-1 px-3 pt-2.5 pb-0">
				<div className="flex items-start justify-between gap-1.5">
					<CardTitle
						className="min-w-0 flex-1 truncate text-sm leading-tight"
						title={node.name}
					>
						{node.name}
					</CardTitle>
					<ReachabilityBadge reachable={node.reachable} />
					{node.supports_mining && (
						<Badge
							variant="secondary"
							className="h-5 rounded-full border-amber-500/40 bg-amber-500/10 px-2 py-0.5 text-xs font-medium text-amber-600 dark:text-amber-400"
						>
							Miner
						</Badge>
					)}
				</div>
				<Tooltip>
					<TooltipTrigger asChild>
						<CardDescription className="truncate text-xs font-medium">
							{node.description}
						</CardDescription>
					</TooltipTrigger>
					<TooltipContent side="top">{node.description}</TooltipContent>
				</Tooltip>

				<div className="flex flex-wrap items-center gap-1">
					<Badge variant="outline" className={compactBadgeClass}>
						{node.implementation}
					</Badge>
					{node.version && (
						<Badge
							variant="outline"
							className={`${compactBadgeClass} max-w-full truncate font-mono`}
						>
							{node.version}
						</Badge>
					)}
					<NodeMetric label="Height" value={activeHeight || "N/A"} />
					<NodeMetric label="Lag" value={lag} />
				</div>
			</CardHeader>

			<CardContent className="space-y-1.5 px-3 pt-2 pb-2.5">
				{statusSummary.length > 0 && (
					<ul className="flex flex-wrap gap-1">
						{statusSummary.map(([status, count]) => (
							<li key={status} className="max-w-full">
								<Tooltip>
									<TooltipTrigger asChild>
										<Badge
											variant="outline"
											className={`${compactBadgeClass} max-w-full justify-start`}
										>
											<span
												className="h-1.5 w-1.5 shrink-0 rounded-full ring-1 ring-background/70"
												style={{ backgroundColor: TIP_STATUS_COLORS[status] }}
												aria-hidden="true"
											/>
											<span>{TIP_STATUS_LABELS[status]}</span>
											<span className="inline-flex size-4 shrink-0 items-center justify-center rounded-full bg-muted text-xs leading-none">
												{count}
											</span>
										</Badge>
									</TooltipTrigger>
									<TooltipContent side="top" className="max-w-64">
										{TIP_STATUS_DESCRIPTIONS[status]}
									</TooltipContent>
								</Tooltip>
							</li>
						))}
					</ul>
				)}
			</CardContent>
		</Card>
	);
}
