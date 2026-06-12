import { ToggleButton, ToggleButtonGroup } from '@mui/material'
import { RANGE_PRESETS, type RangePreset } from '../../chartData'

/** Yahoo-style chart range presets (1M … MAX). Re-clicking the active preset
 *  is a no-op (MUI reports it as null). */
export function RangeToggle({
  value,
  onChange,
}: {
  value: RangePreset
  onChange: (next: RangePreset) => void
}) {
  return (
    <ToggleButtonGroup
      size="small"
      exclusive
      value={value}
      onChange={(_event, next: RangePreset | null) => {
        if (next !== null) onChange(next)
      }}
      aria-label="price range"
      sx={{ mb: 1 }}
    >
      {RANGE_PRESETS.map((preset) => (
        <ToggleButton key={preset} value={preset}>
          {preset}
        </ToggleButton>
      ))}
    </ToggleButtonGroup>
  )
}
