import { Table, TableBody, TableCell, TableHead, TableRow, Typography } from '@mui/material'
import { formatNum, formatPct } from '../format'
import type { PeerRow } from '../types'

export function PeersPanel({ peers }: { peers: PeerRow[] }) {
  if (peers.length === 0) {
    return <Typography color="text.secondary">No peers in the same sector.</Typography>
  }
  return (
    <Table size="small">
      <TableHead>
        <TableRow>
          <TableCell>Ticker</TableCell>
          <TableCell>Name</TableCell>
          <TableCell align="right">Graham score</TableCell>
          <TableCell align="right">Graham number</TableCell>
          <TableCell align="right">Margin of safety</TableCell>
        </TableRow>
      </TableHead>
      <TableBody>
        {peers.map(({ company, score }) => (
          <TableRow key={company.ticker} hover>
            <TableCell>{company.ticker}</TableCell>
            <TableCell>{company.name}</TableCell>
            <TableCell align="right">{score?.score ?? '—'}</TableCell>
            <TableCell align="right">{score ? formatNum(score.graham_number) : '—'}</TableCell>
            <TableCell align="right">{score ? formatPct(score.margin_of_safety) : '—'}</TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  )
}
