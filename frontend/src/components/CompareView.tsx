import { useEffect, useState } from 'react'
import { Autocomplete, Box, Chip, Stack, TextField, Typography } from '@mui/material'
import { listCompanies, loadCompanyData } from '../api'
import type { CompanyData, CompanyRow } from '../types'
import { Compare, type CompareRow } from './Compare'
import { Skeleton } from './Skeleton'

function latestMetrics(d: CompanyData): Record<string, number> {
  const m: Record<string, number> = {}
  for (const r of d.ratios) {
    if (r.period_type === 'annual') m[r.metric] = r.value
  }
  return m
}

export function CompareView() {
  const [tickers, setTickers] = useState<string[]>([])
  const [rows, setRows] = useState<CompareRow[]>([])
  const [inputValue, setInputValue] = useState('')
  const [options, setOptions] = useState<CompanyRow[]>([])
  const [loading, setLoading] = useState(false)

  useEffect(() => {
    if (!inputValue.trim()) {
      // eslint-disable-next-line react-hooks/set-state-in-effect
      setOptions([])
      return
    }
    const id = setTimeout(() => {
      void listCompanies(inputValue, null, 'asc', 8, 0).then((p) => setOptions(p.rows))
    }, 300)
    return () => clearTimeout(id)
  }, [inputValue])

  async function addTicker(row: CompanyRow | null) {
    if (!row) return
    const ticker = row.company.ticker
    if (tickers.includes(ticker)) return
    setTickers((prev) => [...prev, ticker])
    setLoading(true)
    try {
      const d = await loadCompanyData(ticker)
      setRows((prev) => [...prev, { ticker, metrics: latestMetrics(d) }])
    } finally {
      setLoading(false)
    }
  }

  function removeTicker(ticker: string) {
    setTickers((prev) => prev.filter((t) => t !== ticker))
    setRows((prev) => prev.filter((r) => r.ticker !== ticker))
  }

  return (
    <Box>
      <Autocomplete<CompanyRow>
        options={options}
        getOptionLabel={(o) => `${o.company.ticker} — ${o.company.name}`}
        isOptionEqualToValue={(o, v) => o.company.ticker === v.company.ticker}
        filterOptions={(x) => x}
        inputValue={inputValue}
        onInputChange={(_e, v, reason) => {
          if (reason !== 'reset') setInputValue(v)
        }}
        onChange={(_e, v) => {
          void addTicker(v)
          setInputValue('')
        }}
        renderInput={(params) => (
          <TextField {...params} label="Search ticker or name" size="small" />
        )}
        sx={{ mb: 2 }}
      />
      {tickers.length > 0 && (
        <Stack direction="row" spacing={1} sx={{ mb: 2, flexWrap: 'wrap' }}>
          {tickers.map((t) => (
            <Chip key={t} label={t} onDelete={() => removeTicker(t)} />
          ))}
        </Stack>
      )}
      {loading ? (
        <Skeleton />
      ) : tickers.length === 0 ? (
        <Typography color="text.secondary">Add tickers above to compare.</Typography>
      ) : (
        <Compare rows={rows} />
      )}
    </Box>
  )
}
