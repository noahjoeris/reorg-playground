import { Avatar, AvatarFallback, AvatarImage } from '@/components/ui/avatar'
import { Button } from '@/components/ui/button'
import { Separator } from '@/components/ui/separator'
import type { ThemePreference } from '@/hooks/useTheme'
import { ConnectionStatus } from './ConnectionStatus'
import { NetworkSelector } from './NetworkSelector'
import { ThemeToggle } from './ThemeToggle'
import type { ConnectionStatus as ConnectionState, Network } from './types'

const REPO_URL = 'https://github.com/noahjoeris/reorg-playground'

function MetricsDivider() {
  return <Separator orientation="vertical" className="h-3.5 bg-border/70" />
}

export function AppHeader({
  networks,
  selectedNetworkId,
  onNetworkChange,
  themePreference,
  onCycleTheme,
  blockCount,
  reachableNodes,
  totalNodes,
  connectionStatus,
}: {
  networks: Network[]
  selectedNetworkId: number | null
  onNetworkChange: (id: number) => void
  themePreference: ThemePreference
  onCycleTheme: () => void
  blockCount: number
  reachableNodes: number
  totalNodes: number
  connectionStatus: ConnectionState
}) {
  return (
    <header className="relative z-20 px-2.5 pt-2 pb-1.5 sm:px-3.5 sm:pt-2.5 lg:px-5 lg:pt-3">
      <div className="panel-glass-strong rounded-2xl px-3 py-2 sm:px-4 sm:py-2.5">
        <div className="flex flex-col gap-1.5 sm:flex-row sm:items-start sm:justify-between">
          <div className="flex min-w-0 items-center gap-2.5">
            <Avatar className="size-16 sm:size-16">
              <AvatarImage src="/logo.webp" alt="Reorg Playground logo" className="object-cover" />
              <AvatarFallback className="rounded-lg">RP</AvatarFallback>
            </Avatar>
            <div>
              <h1 className="font-display text-lg font-semibold tracking-wide text-foreground sm:text-xl">
                Reorg Playground
              </h1>
              <p className="mt-0.5 hidden max-w-2xl text-xs leading-relaxed text-muted-foreground sm:block">
                Watch how nodes perceive forks, tips, and reorg events in real time.
              </p>
            </div>
          </div>

          <div className="flex shrink-0 items-center gap-2">
            <NetworkSelector networks={networks} selectedId={selectedNetworkId} onChange={onNetworkChange} />
            <ThemeToggle preference={themePreference} onToggle={onCycleTheme} />
            <Button asChild variant="ghost" size="icon-sm">
              <a
                href={REPO_URL}
                target="_blank"
                rel="noreferrer"
                aria-label="Open GitHub repository"
                title="Open GitHub repository"
              >
                <img src="/icons/github.svg" alt="" className="size-4 dark:invert" />
              </a>
            </Button>
          </div>
        </div>

        <div className="mt-1.5 flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-muted-foreground">
          <span className="rounded-full border border-border/80 bg-card/70 px-2.5 py-px text-xs tracking-wide">
            <span>{blockCount.toLocaleString()}</span> blocks
          </span>
          <MetricsDivider />
          <span className="rounded-full border border-border/80 bg-card/70 px-2.5 py-px text-xs tracking-wide">
            {reachableNodes}/{totalNodes} nodes reachable
          </span>
          <MetricsDivider />
          <ConnectionStatus status={connectionStatus} />
        </div>
      </div>
    </header>
  )
}
