type TierBadgeProps = {
  tier: 1 | 2 | 3 | null | undefined
  size?: 'sm' | 'md'
}

const tierConfig = {
  1: { color: 'text-amber-400', label: 'Tier 1', symbol: '★' },
  2: { color: 'text-gray-300', label: 'Tier 2', symbol: '★' },
  3: { color: 'text-orange-600', label: 'Tier 3', symbol: '★' },
}

export function TierBadge({ tier, size = 'sm' }: TierBadgeProps) {
  if (tier === null || tier === undefined) return null

  const config = tierConfig[tier]
  const sizeClasses = size === 'md' ? 'text-base' : 'text-sm'

  return (
    <span className={`${sizeClasses} ${config.color}`} title={config.label}>
      {config.symbol}
    </span>
  )
}
