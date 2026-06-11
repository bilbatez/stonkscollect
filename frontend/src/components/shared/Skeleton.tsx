import { Box, CircularProgress, Typography } from '@mui/material'

/** Loading placeholder. */
export function Skeleton({ label = 'Loading…' }: { label?: string }) {
  return (
    <Box
      role="status"
      aria-live="polite"
      sx={{ display: 'flex', alignItems: 'center', gap: 1.5, p: 3, color: 'text.secondary' }}
    >
      <CircularProgress size={20} />
      <Typography component="span" variant="body2">
        {label}
      </Typography>
    </Box>
  )
}
