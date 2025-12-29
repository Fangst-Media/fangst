export type MonitoringPreset = 'all' | 'future' | 'missing' | 'latest_season'

type Props = {
  value: MonitoringPreset
  onChange: (preset: MonitoringPreset) => void
  seriesStatus?: 'returning' | 'ended' | 'upcoming' | 'unknown'
}

const presets: { value: MonitoringPreset; label: string; description: string }[] = [
  { value: 'all', label: 'All', description: 'Monitor all episodes' },
  { value: 'future', label: 'Future', description: 'Only episodes not yet aired' },
  { value: 'missing', label: 'Missing', description: 'All episodes (find gaps)' },
  { value: 'latest_season', label: 'Latest', description: 'Only the latest season' },
]

export function MonitoringPresetSelector({ value, onChange, seriesStatus }: Props) {
  const isFutureDisabled = seriesStatus === 'ended'

  return (
    <div className="flex items-center gap-2">
      <span className="text-sm text-muted">Monitor:</span>
      <div className="flex gap-1">
        {presets.map((preset) => {
          const isDisabled = preset.value === 'future' && isFutureDisabled
          const isSelected = value === preset.value

          return (
            <button
              key={preset.value}
              type="button"
              onClick={() => !isDisabled && onChange(preset.value)}
              disabled={isDisabled}
              title={isDisabled ? 'Series has ended' : preset.description}
              className={`px-3 py-1 text-xs font-medium rounded-md transition-colors ${
                isSelected
                  ? 'bg-accent text-white'
                  : isDisabled
                    ? 'bg-card text-muted/50 cursor-not-allowed'
                    : 'bg-card text-muted hover:text-foreground border border-border'
              }`}
            >
              {preset.label}
            </button>
          )
        })}
      </div>
    </div>
  )
}
