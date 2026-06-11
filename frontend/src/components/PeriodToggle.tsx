import { ToggleButton, ToggleButtonGroup } from '@mui/material'
import type { Period } from '../types'

/** Annual / Quarterly switch for ratios and statements. */
export function PeriodToggle({
  period,
  onChange,
}: {
  period: Period
  onChange: (p: Period) => void
}) {
  return (
    <ToggleButtonGroup
      size="small"
      exclusive
      value={period}
      aria-label="period"
      onChange={(_e, v: Period | null) => {
        if (v !== null) {
          onChange(v)
        }
      }}
    >
      <ToggleButton value="annual">Annual</ToggleButton>
      <ToggleButton value="quarterly">Quarterly</ToggleButton>
    </ToggleButtonGroup>
  )
}
