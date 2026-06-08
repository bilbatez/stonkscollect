import {
  Card,
  CardContent,
  Chip,
  List,
  ListItem,
  ListItemIcon,
  ListItemText,
  Stack,
  Typography,
} from '@mui/material'
import CheckCircleIcon from '@mui/icons-material/CheckCircle'
import CancelIcon from '@mui/icons-material/Cancel'
import type { GrahamAssessment } from '../types'

function pct(x: number | null): string {
  return x === null ? '—' : `${(x * 100).toFixed(0)}%`
}
function num(x: number | null): string {
  return x === null ? '—' : x.toFixed(2)
}

/** Graham defensive-investor scorecard for one company. */
export function GrahamScorecard({ assessment }: { assessment: GrahamAssessment }) {
  const { criteria, score, passes_defensive, graham_number, margin_of_safety, net_net } = assessment
  return (
    <Card variant="outlined">
      <CardContent>
        <Stack direction="row" spacing={1} sx={{ mb: 1.5, alignItems: 'center', flexWrap: 'wrap' }}>
          <Typography variant="h6" component="h3">
            Graham scorecard
          </Typography>
          <Chip
            size="small"
            color={passes_defensive ? 'success' : 'warning'}
            label={`${score}/${criteria.length}`}
          />
          {passes_defensive && <Chip size="small" color="success" label="defensive" />}
          {net_net && <Chip size="small" color="primary" label="net-net" />}
        </Stack>

        <List dense disablePadding>
          {criteria.map((c) => (
            <ListItem key={c.name} disableGutters>
              <ListItemIcon sx={{ minWidth: 36 }}>
                {c.passed ? (
                  <CheckCircleIcon color="success" aria-label="pass" />
                ) : (
                  <CancelIcon color="error" aria-label="fail" />
                )}
              </ListItemIcon>
              <ListItemText primary={c.name} secondary={c.detail} />
            </ListItem>
          ))}
        </List>

        <Stack direction="row" spacing={4} sx={{ mt: 1.5 }}>
          <Stack>
            <Typography variant="caption" color="text.secondary">
              Graham Number
            </Typography>
            <Typography variant="body1">{num(graham_number)}</Typography>
          </Stack>
          <Stack>
            <Typography variant="caption" color="text.secondary">
              Margin of safety
            </Typography>
            <Typography variant="body1">{pct(margin_of_safety)}</Typography>
          </Stack>
        </Stack>
      </CardContent>
    </Card>
  )
}
