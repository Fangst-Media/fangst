import { useState } from 'react'
import { setApiKey } from '../../lib/dl-api'

type ApiKeyFormProps = {
  reason?: string
  onDone?: () => void
}

export function ApiKeyForm({ reason, onDone }: ApiKeyFormProps) {
  const [value, setValue] = useState('')

  return (
    <form
      className="max-w-md rounded-xl border border-border bg-card p-6"
      onSubmit={(event) => {
        event.preventDefault()
        const key = value.trim()
        if (key === '') return
        setApiKey(key)
        setValue('')
        onDone?.()
      }}
    >
      <h2 className="text-lg font-semibold text-foreground">Download daemon API key</h2>
      <p className="mt-1 text-sm text-muted">
        {reason ??
          'Enter the FANGST_API_KEY the daemon was started with. It is stored in this browser only.'}
      </p>
      <label className="mt-4 block text-sm font-medium text-foreground" htmlFor="dl-api-key">
        API key
      </label>
      <input
        id="dl-api-key"
        type="password"
        autoComplete="off"
        value={value}
        onChange={(event) => setValue(event.target.value)}
        className="mt-1 w-full rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground focus:border-accent focus:outline-none"
      />
      <div className="mt-4 flex gap-2">
        <button
          type="submit"
          disabled={value.trim() === ''}
          className="rounded-md bg-accent px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-accent/90 disabled:opacity-50"
        >
          Save key
        </button>
        {onDone && (
          <button
            type="button"
            onClick={onDone}
            className="rounded-md border border-border px-4 py-2 text-sm font-medium text-muted transition-colors hover:text-foreground"
          >
            Cancel
          </button>
        )}
      </div>
    </form>
  )
}
