type QualityScoreProps = {
  score: number
  size?: 'sm' | 'md'
}

function getScoreColor(score: number): string {
  if (score >= 80) return 'text-amber-400 border-amber-400/50'
  if (score >= 60) return 'text-blue-400 border-blue-400/50'
  if (score >= 40) return 'text-green-400 border-green-400/50'
  return 'text-gray-400 border-gray-400/50'
}

export function QualityScore({ score, size = 'sm' }: QualityScoreProps) {
  const sizeClasses = size === 'md' ? 'w-9 h-9 text-xs' : 'w-7 h-7 text-[10px]'

  return (
    <div
      className={`${sizeClasses} rounded-full border-2 flex items-center justify-center font-bold ${getScoreColor(score)}`}
      title={`Quality score: ${score}/100`}
    >
      {score}
    </div>
  )
}
