import { useEffect, useState } from 'react'
import {
  Paper,
  Table,
  TableBody,
  TableCell,
  TableContainer,
  TableHead,
  TableRow,
  Typography,
} from '@mui/material'
import { getHolders } from '../../api'
import { formatCompact, formatDateTime } from '../../format'
import type { OwnershipHolding } from '../../types'

/** Insider/institutional holders for a company (e.g. SEC Form 4 positions),
 *  fetched on demand by ticker. Newest filing first. */
export function HoldersPanel({ ticker }: { ticker: string }) {
  const [holders, setHolders] = useState<OwnershipHolding[]>([])
  useEffect(() => {
    getHolders(ticker)
      .then(setHolders)
      .catch(() => {})
  }, [ticker])
  if (holders.length === 0) {
    return <Typography color="text.secondary">No holder data.</Typography>
  }
  return (
    <TableContainer component={Paper} variant="outlined">
      <Table size="small">
        <TableHead>
          <TableRow>
            <TableCell>Holder</TableCell>
            <TableCell>Type</TableCell>
            <TableCell align="right">Shares</TableCell>
            <TableCell align="right">As of</TableCell>
          </TableRow>
        </TableHead>
        <TableBody>
          {holders.map((h) => (
            <TableRow key={`${h.holder}-${h.as_of}`} hover>
              <TableCell>{h.holder}</TableCell>
              <TableCell sx={{ textTransform: 'capitalize' }}>{h.kind}</TableCell>
              <TableCell align="right">{formatCompact(h.shares)}</TableCell>
              <TableCell align="right">{formatDateTime(h.as_of)}</TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </TableContainer>
  )
}
