import {
  Box,
  Button,
  Checkbox,
  FormControlLabel,
  Paper,
  Stack,
  Table,
  TableBody,
  TableCell,
  TableContainer,
  TableHead,
  TableRow,
  Typography,
} from '@mui/material'
import type { ScreenRow } from '../types'

interface Props {
  rows: ScreenRow[]
  defensiveOnly: boolean
  onToggleDefensive: () => void
  onSelect: (ticker: string) => void
}

/** Graham screener results, ranked by score. */
export function Screener({ rows, defensiveOnly, onToggleDefensive, onSelect }: Props) {
  return (
    <Box>
      <Stack
        direction="row"
        spacing={2}
        sx={{ mb: 2, alignItems: 'center', justifyContent: 'space-between' }}
      >
        <Typography variant="h5" component="h2">
          Screener
        </Typography>
        <FormControlLabel
          control={<Checkbox checked={defensiveOnly} onChange={onToggleDefensive} />}
          label="Defensive only"
        />
      </Stack>
      {rows.length === 0 ? (
        <Typography color="text.secondary">No matches.</Typography>
      ) : (
        <TableContainer component={Paper} variant="outlined">
          <Table size="small">
            <TableHead>
              <TableRow>
                <TableCell>Ticker</TableCell>
                <TableCell align="right">Score</TableCell>
                <TableCell align="right">Graham #</TableCell>
                <TableCell align="right">Margin</TableCell>
              </TableRow>
            </TableHead>
            <TableBody>
              {rows.map((r) => (
                <TableRow key={r.company.ticker} hover>
                  <TableCell>
                    <Button size="small" onClick={() => onSelect(r.company.ticker)}>
                      {r.company.ticker}
                    </Button>
                  </TableCell>
                  <TableCell align="right">{r.score.score}</TableCell>
                  <TableCell align="right">
                    {r.score.graham_number === null ? '—' : r.score.graham_number.toFixed(2)}
                  </TableCell>
                  <TableCell align="right">
                    {r.score.margin_of_safety === null
                      ? '—'
                      : `${(r.score.margin_of_safety * 100).toFixed(0)}%`}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </TableContainer>
      )}
    </Box>
  )
}
